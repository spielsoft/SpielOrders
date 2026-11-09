use std::{
    fs,
    path::{Path, PathBuf},
};

use pulldown_cmark::{Event, Parser, Tag};

use crate::Result;

pub(crate) fn check(root: &Path) -> Result<()> {
    let mut documents = markdown_files(&root.join("issues"))?;
    documents.extend(markdown_files(&root.join("docs"))?);
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "md") {
            documents.push(entry.path());
        }
    }
    documents.sort();
    let mut links = 0;
    for document in documents {
        let text = fs::read_to_string(&document)?;
        for event in Parser::new(&text) {
            let Event::Start(Tag::Link { dest_url, .. }) = event else {
                continue;
            };
            let target = dest_url.split('#').next().unwrap_or_default();
            if target.is_empty() || has_scheme(target) {
                continue;
            }
            let target = decode_path(target)?;
            let path = document
                .parent()
                .expect("document has a parent")
                .join(&target);
            if !path.try_exists()? {
                return Err(format!("{}: missing link target {target}", document.display()).into());
            }
            links += 1;
        }
    }
    let decisions = root.join("issues/wayfinder/spielorders-product-architecture/tickets");
    for decision in markdown_files(&decisions)? {
        let text = fs::read_to_string(&decision)?;
        let status = text
            .strip_prefix("---\n")
            .and_then(|body| body.split_once("\n---"))
            .and_then(|(header, _)| {
                header
                    .lines()
                    .find_map(|line| line.strip_prefix("status: "))
            });
        if status != Some("closed") {
            return Err(format!("{}: governing decision is not closed", decision.display()).into());
        }
    }
    println!("documentation checks passed for {links} local links");
    Ok(())
}

fn markdown_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if !entry.file_name().to_string_lossy().starts_with('.') {
                files.extend(markdown_files(&entry.path())?);
            }
        } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "md") {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

fn has_scheme(target: &str) -> bool {
    target.split_once(':').is_some_and(|(scheme, _)| {
        scheme.starts_with(|character: char| character.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "+.-".contains(character))
    })
}

fn decode_path(target: &str) -> Result<String> {
    let mut bytes = Vec::new();
    let mut remaining = target.as_bytes();
    while let Some((&first, rest)) = remaining.split_first() {
        if first == b'%' {
            let encoded = rest
                .get(..2)
                .ok_or("incomplete URL escape in Markdown link")?;
            bytes.push(u8::from_str_radix(std::str::from_utf8(encoded)?, 16)?);
            remaining = &rest[2..];
        } else {
            bytes.push(first);
            remaining = rest;
        }
    }
    Ok(String::from_utf8(bytes)?)
}
