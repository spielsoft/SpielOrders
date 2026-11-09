//! This executable's own native services: its Supporting Material query and its Trash
//! move. The library performs neither; the desktop has its own.
//!
//! On Apple platforms the query is Spotlight, through the operating system's own
//! `mdutil` and `mdfind`, and the Trash is `NSFileManager`'s. Elsewhere this build has
//! neither, and says so rather than falling back to something weaker.
//!
//! Decision sources: Define the Rust CLI capability contract; Define lookup and search
//! behavior; Effect and adapter boundaries.

use crate::library::{c_path, c_text};
use spielorders::{
    PathBytes, SPIELORDERS_RESULT_OK, SpielordersConstant, SpielordersHits, Text,
    spielorders_hits_add, spielorders_hits_diagnostic, spielorders_hits_new,
};
use std::{
    io,
    path::{Path, PathBuf},
    time::Duration,
};

/// The Supporting Material provider this build has, in its own name, or `None` where
/// its platform has none.
#[cfg(target_vendor = "apple")]
pub(crate) const PROVIDER: Option<&str> = Some("spotlight");
#[cfg(not(target_vendor = "apple"))]
pub(crate) const PROVIDER: Option<&str> = None;

/// One file a provider reported, with the facts it stated about it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Hit {
    pub(crate) path: PathBuf,
    pub(crate) kind: Option<String>,
    pub(crate) modified_unix_seconds: Option<i64>,
}

/// What a provider answered: whether it could look, as a `SPIELORDERS_PROVIDER_STATE_*`,
/// what it found, and what it said about the query in its own words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Found {
    pub(crate) state: SpielordersConstant,
    pub(crate) hits: Vec<Hit>,
    pub(crate) said: Vec<String>,
}

/// What this build's provider found for `text` over `directory`, within `bound`, as the
/// hits the library groups, or `None` where this build has no provider. `directory` is
/// the one the library names, and `None` there is a Package with nowhere for Supporting
/// Material to be, which is an answer that found nothing rather than a query.
pub(crate) fn search_supporting_material(
    directory: Option<&Path>,
    text: &str,
    bound: Duration,
) -> Option<(Box<SpielordersHits>, SpielordersConstant)> {
    #[cfg(target_vendor = "apple")]
    let found = spotlight::answer(directory, text, bound);
    #[cfg(not(target_vendor = "apple"))]
    let found: Found = {
        let _ = (directory, text, bound);
        return None;
    };
    hits(PROVIDER?, &found).map(|hits| (hits, found.state))
}

/// What `provider` found, gathered through the library's interface.
pub(crate) fn hits(provider: &str, found: &Found) -> Option<Box<SpielordersHits>> {
    let provider = c_text(provider);
    let mut hits = None;
    if spielorders_hits_new(Text::new(&provider), &mut hits) != SPIELORDERS_RESULT_OK {
        return None;
    }
    let mut hits = hits?;
    for hit in &found.hits {
        let path = c_path(&hit.path);
        let kind = c_text(hit.kind.clone().unwrap_or_default());
        let modified = c_text(
            hit.modified_unix_seconds
                .map(|seconds| seconds.to_string())
                .unwrap_or_default(),
        );
        // The query prints paths and nothing else, so it states no relevance.
        let _ = spielorders_hits_add(
            &mut hits,
            PathBytes::new(&path),
            Text::new(&kind),
            Text::new(&modified),
            Text::new(c""),
        );
    }
    for said in &found.said {
        let said = c_text(said.as_str());
        let _ = spielorders_hits_diagnostic(&mut hits, Text::new(&said));
    }
    Some(hits)
}

/// Moves one complete directory to the platform Trash and reports where it landed, when
/// the platform says. It never erases anything: where this build has no Trash it refuses.
pub(crate) fn trash(directory: &Path) -> io::Result<Option<PathBuf>> {
    trash::directory(directory)
}

#[cfg(not(target_vendor = "apple"))]
mod trash {
    /// Refusing is the only honest answer where this build has no Trash to ask: a
    /// directory it cannot trash stays where the removal put it, recoverable.
    pub(super) fn directory(_: &std::path::Path) -> std::io::Result<Option<std::path::PathBuf>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "this platform has no Trash this build can move a directory to",
        ))
    }
}

#[cfg(target_vendor = "apple")]
mod trash {
    use objc2_foundation::{NSFileManager, NSString, NSURL};
    use std::{
        io,
        path::{Path, PathBuf},
    };

    /// **The platform's own request to trash an item**, `NSFileManager`'s
    /// `trashItemAtURL:resultingItemURL:error:`: it moves the item whole, renames a
    /// colliding one rather than replacing it, and reports the name it gave it. Moving the
    /// directory into `~/.Trash` ourselves would be this application deciding where a
    /// user's Trash is, which is the platform's to decide.
    pub(super) fn directory(path: &Path) -> io::Result<Option<PathBuf>> {
        let path = path
            .to_str()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a path is not UTF-8"))?;
        let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(path), true);
        let mut landed = None;
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, Some(&mut landed))
            .map_err(|error| io::Error::other(error.localizedDescription().to_string()))?;
        Ok(landed
            .and_then(|landed| landed.path())
            .map(|landed| PathBuf::from(landed.to_string())))
    }
}

#[cfg(target_vendor = "apple")]
mod spotlight {
    use super::{Found, Hit};
    use spielorders::{
        SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED, SPIELORDERS_PROVIDER_STATE_COMPLETE,
        SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED, SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED,
        SPIELORDERS_PROVIDER_STATE_TIMED_OUT, SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED,
    };
    use std::{
        ffi::OsStr,
        io::{self, Read},
        os::unix::{ffi::OsStrExt, process::ExitStatusExt},
        path::{Path, PathBuf},
        process::{Child, Command, ExitStatus, Stdio},
        sync::{Arc, Mutex},
        thread::JoinHandle,
        time::{Duration, Instant},
    };

    /// The operating system's own metadata tools, by absolute path, so the tool asked is
    /// the one the system ships rather than whatever `PATH` resolves. Processes rather than
    /// bindings: both questions are ones the shipped tools already answer.
    const STATE_PROGRAM: &str = "/usr/bin/mdutil";
    const QUERY_PROGRAM: &str = "/usr/bin/mdfind";

    /// How long the wait between two looks at the bound is. A choice: short enough that a
    /// query that outlives its bound is not waited on long past it.
    const LOOK: Duration = Duration::from_millis(25);

    /// The signal a stopped tool ends with.
    const SIGKILL: i32 = 9;

    /// How long a stopped tool's streams are read for what it wrote before it stopped. A
    /// choice: the pipe holds it already, so this only lets the readers catch up.
    const DRAIN: Duration = Duration::from_millis(250);

    pub(super) fn answer(directory: Option<&Path>, text: &str, bound: Duration) -> Found {
        let found = |state, hits| Found {
            state,
            hits,
            said: Vec::new(),
        };
        let prevented = |state, said: Option<String>| Found {
            state,
            hits: Vec::new(),
            said: said.into_iter().collect(),
        };
        let Some(directory) = directory else {
            return found(SPIELORDERS_PROVIDER_STATE_COMPLETE, Vec::new());
        };
        // **Asked of the filesystem, because the filesystem is what knows.** The query
        // prints nothing and succeeds for a directory this process cannot read, so a
        // refusal is settled first; it is the narrower fact than the volume's index state.
        if let Err(refusal) = std::fs::read_dir(directory)
            && refusal.kind() == io::ErrorKind::PermissionDenied
        {
            return prevented(
                SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED,
                Some(format!(
                    "{} could not be read: {refusal}",
                    directory.display()
                )),
            );
        }
        // **Before the query, because the query cannot report these.** It prints nothing
        // and succeeds whether the index is switched off, the volume is unindexed, or
        // nothing matched.
        let asking = Instant::now();
        match state(directory, bound) {
            IndexState::Disabled => {
                return prevented(SPIELORDERS_PROVIDER_STATE_INDEXING_DISABLED, None);
            }
            IndexState::NotIndexed => {
                return prevented(SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED, None);
            }
            // A volume the platform will not report on is the same state to a caller as
            // one it says has no index; what the platform said travels beside it.
            IndexState::Unreported(said) => {
                return prevented(SPIELORDERS_PROVIDER_STATE_VOLUME_NOT_INDEXED, Some(said));
            }
            IndexState::Enabled => {}
        }
        // A directory the user excluded from indexing is reported by neither tool, so it
        // arrives as an empty answer, which the search decision's own hedge covers.
        let left = bound.saturating_sub(asking.elapsed());
        match query(directory, text, left) {
            Err(error) => prevented(
                SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED,
                Some(error.to_string()),
            ),
            Ok(ToolRun::Exited { status, said, .. }) if !status.success() => prevented(
                SPIELORDERS_PROVIDER_STATE_QUERY_REFUSED,
                Some(refusal(&status, &said)),
            ),
            Ok(ToolRun::Exited { wrote, .. }) => {
                found(SPIELORDERS_PROVIDER_STATE_COMPLETE, hits(&wrote))
            }
            Ok(ToolRun::Stopped { wrote }) => {
                found(SPIELORDERS_PROVIDER_STATE_TIMED_OUT, hits(&wrote))
            }
        }
    }

    /// What state the platform says the index covering one directory is in.
    #[derive(Debug, Eq, PartialEq)]
    enum IndexState {
        Enabled,
        Disabled,
        NotIndexed,
        /// The platform would not say, in its own words.
        Unreported(String),
    }

    /// **`mdutil` reports on a mount point**, so it is asked about the volume this
    /// directory is on: handed any other path, it answers "unknown indexing state".
    fn state(directory: &Path, bound: Duration) -> IndexState {
        let ran = enclosing_volume(directory)
            .and_then(|volume| bounded(Command::new(STATE_PROGRAM).arg("-s").arg(volume), bound));
        match ran {
            // **The status is not the answer, so the output is.** The tool exits
            // successfully having printed that its path was invalid.
            Ok(ToolRun::Exited { status, wrote, .. }) => {
                match index_state(&String::from_utf8_lossy(&wrote)) {
                    IndexState::Unreported(said) if said.is_empty() => {
                        IndexState::Unreported(status.to_string())
                    }
                    state => state,
                }
            }
            Ok(ToolRun::Stopped { .. }) => IndexState::Unreported(
                "the platform did not report this volume's index state within the time it \
                 was given"
                    .to_owned(),
            ),
            Err(error) => IndexState::Unreported(error.to_string()),
        }
    }

    /// What one `mdutil -s` report says, as the state it names.
    ///
    /// **Only the indented lines are read**, because the tool prints the volume it
    /// resolved on a line of its own and a mount point spelled with one of these words
    /// would otherwise decide the state. **A report naming both readings is read as the
    /// disabling**, because an index that will not be searched answers nothing: a choice,
    /// not a rule. A phrasing this does not know settles as the volume's state rather than
    /// as an answer, so it still stops a run from reporting a search that completed.
    fn index_state(reported: &str) -> IndexState {
        let said = reported
            .lines()
            .filter(|line| line.starts_with(char::is_whitespace))
            .map(str::trim)
            .collect::<Vec<_>>()
            .join(" ");
        let named = said.to_ascii_lowercase();
        if named.contains("disabled") {
            IndexState::Disabled
        } else if named.contains("enabled") {
            IndexState::Enabled
        } else if named.contains("no index") {
            IndexState::NotIndexed
        } else if said.is_empty() {
            IndexState::Unreported(reported.trim().to_owned())
        } else {
            IndexState::Unreported(said)
        }
    }

    /// The mount point of the volume `path` is on, through `statfs`.
    fn enclosing_volume(path: &Path) -> io::Result<PathBuf> {
        let reported = rustix::fs::statfs(path)?;
        let mount: Vec<u8> = reported
            .f_mntonname
            .iter()
            .take_while(|byte| **byte != 0)
            .map(|byte| byte.to_ne_bytes()[0])
            .collect();
        Ok(PathBuf::from(OsStr::from_bytes(&mount)))
    }

    /// The query over `scope`, NUL-separated so a filename holding a newline stays one
    /// path. The text is the caller's own, passed last and whole: the provider governs
    /// matching, so nothing here tokenizes, folds, or wraps it.
    fn query(scope: &Path, text: &str, bound: Duration) -> io::Result<ToolRun> {
        bounded(
            Command::new(QUERY_PROGRAM)
                .arg("-0")
                .arg("-onlyin")
                .arg(scope)
                .arg(text),
            bound,
        )
    }

    /// What the query wrote, as one hit per path it reported, with no narrowing of its
    /// own: which paths are this Package's Supporting Material is the library's to decide.
    /// The tail after the final terminator is a path the query never finished.
    fn hits(output: &[u8]) -> Vec<Hit> {
        let mut fields: Vec<&[u8]> = output.split(|byte| *byte == 0).collect();
        fields.pop();
        fields
            .into_iter()
            .filter(|field| !field.is_empty())
            .map(|field| {
                let path = PathBuf::from(OsStr::from_bytes(field));
                Hit {
                    kind: extension_kind(&path),
                    modified_unix_seconds: modified_unix_seconds(&path),
                    path,
                }
            })
            .collect()
    }

    /// The file's kind, taken from its name's extension: what a reader sees in Finder,
    /// without a second query per result. No extension is no kind.
    fn extension_kind(path: &Path) -> Option<String> {
        path.extension()
            .and_then(OsStr::to_str)
            .filter(|extension| !extension.is_empty())
            .map(str::to_ascii_lowercase)
    }

    /// When the file was last written, signed so a file stamped before 1970 is reported
    /// as such; absent rather than guessed where the path cannot be read.
    fn modified_unix_seconds(path: &Path) -> Option<i64> {
        let modified = std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()?;
        match modified.duration_since(std::time::UNIX_EPOCH) {
            Ok(since) => i64::try_from(since.as_secs()).ok(),
            Err(before) => i64::try_from(before.duration().as_secs())
                .ok()
                .map(|seconds| -seconds),
        }
    }

    /// How one bounded run of a platform tool ended.
    #[derive(Debug)]
    enum ToolRun {
        /// The tool exited on its own, with what it wrote to each stream.
        Exited {
            status: ExitStatus,
            wrote: Vec<u8>,
            said: Vec<u8>,
        },
        /// The bound elapsed first, and this is what it had written by then.
        Stopped { wrote: Vec<u8> },
    }

    /// One stream of a running tool, gathered by a thread of its own as it arrives.
    struct Gathered {
        bytes: Arc<Mutex<Vec<u8>>>,
        reader: Option<JoinHandle<()>>,
    }

    impl Gathered {
        fn new(stream: Option<impl Read + Send + 'static>) -> Self {
            let bytes = Arc::new(Mutex::new(Vec::new()));
            let reader = stream.map(|mut stream| {
                let bytes = Arc::clone(&bytes);
                std::thread::spawn(move || {
                    let mut buffer = [0; 8192];
                    while let Ok(length @ 1..) = stream.read(&mut buffer) {
                        if let Ok(mut gathered) = bytes.lock() {
                            gathered.extend_from_slice(&buffer[..length]);
                        }
                    }
                })
            });
            Self { bytes, reader }
        }

        fn finished(&self) -> bool {
            self.reader.as_ref().is_none_or(JoinHandle::is_finished)
        }

        /// What arrived so far. A reader still waiting on a descendant that holds the
        /// stream is left to the process's exit rather than waited on.
        fn taken(self) -> Vec<u8> {
            self.bytes
                .lock()
                .map(|bytes| bytes.clone())
                .unwrap_or_default()
        }
    }

    /// One run of a platform tool, held to `bound`.
    ///
    /// **Both tools go through it, because the bound is on the answer.** Both streams are
    /// piped, so a tool's locale notice never reaches this process's standard error; the
    /// exit status is the failure signal, and what was said is read back only to carry
    /// the tool's own words into a refusal.
    fn bounded(asking: &mut Command, bound: Duration) -> io::Result<ToolRun> {
        let child = asking
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut running = RunningTool(child);
        let wrote = Gathered::new(running.0.stdout.take());
        let said = Gathered::new(running.0.stderr.take());
        let deadline = Instant::now() + bound;
        let status = loop {
            if let Some(status) = running.0.try_wait()? {
                break status;
            }
            if Instant::now() >= deadline {
                running.0.kill()?;
                let status = running.0.wait()?;
                settle(&wrote, &said, Instant::now() + DRAIN);
                // A tool that finished while being stopped keeps its natural exit.
                if status.signal() == Some(SIGKILL) {
                    return Ok(ToolRun::Stopped {
                        wrote: wrote.taken(),
                    });
                }
                return Ok(ToolRun::Exited {
                    status,
                    wrote: wrote.taken(),
                    said: said.taken(),
                });
            }
            std::thread::sleep(LOOK.min(deadline.saturating_duration_since(Instant::now())));
        };
        // The streams close as the tool exits; a descendant holding one open does not
        // extend the run past its bound.
        settle(&wrote, &said, deadline);
        Ok(ToolRun::Exited {
            status,
            wrote: wrote.taken(),
            said: said.taken(),
        })
    }

    /// Waits until both streams are read to their end, or until `until`.
    fn settle(wrote: &Gathered, said: &Gathered, until: Instant) {
        while !(wrote.finished() && said.finished()) && Instant::now() < until {
            std::thread::sleep(LOOK);
        }
    }

    /// Owns cleanup even if configuring or reading a stream fails.
    struct RunningTool(Child);

    impl Drop for RunningTool {
        fn drop(&mut self) {
            if self.0.kill().is_ok() {
                let _ = self.0.wait();
            } else {
                let _ = self.0.try_wait();
            }
        }
    }

    /// What a refused query said about itself, or the status it refused with where it
    /// said nothing.
    fn refusal(status: &ExitStatus, said: &[u8]) -> String {
        let said = String::from_utf8_lossy(said).trim().to_owned();
        if said.is_empty() {
            status.to_string()
        } else {
            said
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{
            Command, Duration, IndexState, SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED,
            SPIELORDERS_PROVIDER_STATE_COMPLETE, ToolRun, answer, bounded, enclosing_volume, hits,
            index_state, state,
        };
        use crate::platform::Found;
        use std::{os::unix::fs::PermissionsExt, path::PathBuf};

        /// A disposable directory, removed afterwards.
        struct Scratch(PathBuf);

        impl Scratch {
            fn new(label: &str) -> Self {
                let path = std::env::temp_dir().join(format!(
                    "spielorders-cli-platform-{label}-{}",
                    std::process::id()
                ));
                std::fs::create_dir(&path).unwrap();
                Self(path)
            }

            /// An executable script here, run in place of a platform tool.
            fn script(&self, body: &str) -> PathBuf {
                let program = self.0.join("tool");
                std::fs::write(&program, format!("#!/bin/sh\n{body}")).unwrap();
                std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
                program
            }
        }

        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        /// The bound is a deadline on the clock, so the tool marks when it has printed: a run
        /// whose mark is missing was stopped before printing and says nothing about what a
        /// stopped tool keeps, and is asked again with a longer bound. The tool never exits
        /// on its own, so every run is stopped.
        #[test]
        fn a_tool_that_outlives_its_bound_is_stopped_with_what_it_wrote() {
            let scratch = Scratch::new("bound");
            let printed = scratch.0.join("printed");
            let program = scratch.script(&format!(
                "printf '/reported-file\\0/unfinish'\n: > '{}'\nexec /bin/sleep 100000\n",
                printed.display()
            ));
            let mut bound = Duration::from_secs(2);
            let wrote = loop {
                let ran = bounded(&mut Command::new(&program), bound).unwrap();
                let ToolRun::Stopped { wrote } = ran else {
                    panic!("a tool past its bound is stopped: {ran:?}");
                };
                if printed.exists() {
                    break wrote;
                }
                bound *= 2;
            };
            let found: Vec<PathBuf> = hits(&wrote).into_iter().map(|hit| hit.path).collect();
            assert_eq!(
                found,
                [PathBuf::from("/reported-file")],
                "every complete path is kept, and the unfinished tail is not"
            );
        }

        #[test]
        fn a_tool_that_exits_keeps_its_status_and_its_own_words() {
            let scratch = Scratch::new("exit");
            let program = scratch.script("printf 'tool refused' >&2\nexit 7\n");
            let ToolRun::Exited { status, said, .. } =
                bounded(&mut Command::new(&program), Duration::from_secs(10)).unwrap()
            else {
                panic!("a tool that exits is not stopped");
            };
            assert_eq!(status.code(), Some(7));
            assert_eq!(said, b"tool refused");
        }

        #[test]
        fn a_filename_holding_a_newline_is_one_path() {
            let found: Vec<PathBuf> = hits(b"/scope/two\nlines.txt\0/scope/plain.pdf\0")
                .into_iter()
                .map(|hit| hit.path)
                .collect();
            assert_eq!(
                found,
                [
                    PathBuf::from("/scope/two\nlines.txt"),
                    PathBuf::from("/scope/plain.pdf")
                ]
            );
        }

        /// The report the tool prints for a state is read as that state, and a phrasing
        /// this reading does not know settles as the volume's state.
        #[test]
        fn what_the_tool_prints_is_read_as_the_state_it_names() {
            assert_eq!(
                index_state("/:\n\tIndexing enabled. \n"),
                IndexState::Enabled
            );
            assert_eq!(
                index_state("/Volumes/disabled:\n\tIndexing enabled. \n"),
                IndexState::Enabled,
                "the volume the tool names cannot decide the state"
            );
            assert_eq!(
                index_state("/Volumes/Archive:\n\tIndexing and searching disabled.\n"),
                IndexState::Disabled
            );
            assert_eq!(
                index_state("/Volumes/Archive:\n\tNo index.\n"),
                IndexState::NotIndexed
            );
            assert_eq!(
                index_state("Error: invalid path `/nowhere'.\n"),
                IndexState::Unreported("Error: invalid path `/nowhere'.".to_owned())
            );
            assert_eq!(
                index_state("/Volumes/Archive:\n\tSomething never seen.\n"),
                IndexState::Unreported("Something never seen.".to_owned())
            );
        }

        /// The shipped tools resolve the volume an ordinary directory is on and get a
        /// state for it. Which state is a fact about this machine and is not asserted.
        #[test]
        #[ignore = "asks this machine's own index; scripts/smoke-spotlight.sh runs it"]
        fn the_shipped_tools_get_a_state_for_the_volume_a_directory_is_on() {
            let directory = std::env::temp_dir();
            assert!(enclosing_volume(&directory).unwrap().is_dir());
            if let IndexState::Unreported(said) = state(&directory, Duration::from_secs(10)) {
                panic!("no state for {}: {said}", directory.display());
            }
        }

        #[test]
        fn a_directory_this_process_cannot_read_is_denied_before_any_query() {
            let scratch = Scratch::new("denied");
            std::fs::set_permissions(&scratch.0, std::fs::Permissions::from_mode(0o000)).unwrap();
            let answered = answer(Some(&scratch.0), "quote", Duration::from_secs(10));
            assert_eq!(
                answered.state, SPIELORDERS_PROVIDER_STATE_ACCESS_DENIED,
                "{answered:?}"
            );
        }

        #[test]
        fn nowhere_to_look_is_a_complete_answer_that_found_nothing() {
            assert_eq!(
                answer(None, "quote", Duration::from_secs(10)),
                Found {
                    state: SPIELORDERS_PROVIDER_STATE_COMPLETE,
                    hits: Vec::new(),
                    said: Vec::new(),
                }
            );
        }
    }
}
