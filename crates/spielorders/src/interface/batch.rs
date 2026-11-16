//! Many record changes saved as one guarded write. A batch holds the Package as its
//! changes so far leave it: a draft taken from a batch starts from that state, adding it
//! plans it there, and the batch's plan commits every record its changes touched together.

use crate::interface::{
    PathBytes, SPIELORDERS_DRAFT_KIND_ORDER, SPIELORDERS_DRAFT_KIND_VENDOR,
    SPIELORDERS_RESULT_BAD_ARGUMENT, SPIELORDERS_RESULT_INTERNAL, SPIELORDERS_VENDOR_FIELD_NAME,
    SPIELORDERS_VENDOR_SELECTOR_ID, SPIELORDERS_VENDOR_SELECTOR_NAME, SpielordersConstant,
    SpielordersDraft, SpielordersEntries, SpielordersPackage, SpielordersPlan, SpielordersResult,
    Text,
    call::{Session, deliver, fallible, quietly, settle},
    draft::Origin,
    entries::{Entry, Subject},
    records::{contact_facts, item_facts, order_facts, vendor_facts},
};
use crate::{Answer, Failure, batch::Batch, drafting::new_vendor_name, ports::Effects};

/// Record changes planned one after another against one load of a Package.
#[repr(C)]
pub struct SpielordersBatch {
    data: Box<Batch>,
}

/// A new, empty batch over `package` as it was opened.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_package_batch(
    package: &SpielordersPackage,
    batch: &mut Option<Box<SpielordersBatch>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let result = package.healthy(entries).map(|opened| SpielordersBatch {
            data: Box::new(Batch::new(opened.contents.clone())),
        });
        deliver(result, batch)
    })
}

/// A draft of the Order at `directory` as `batch` holds it, saved by adding it to the
/// batch. Refuses SPIELORDERS_FAILURE_PRECONDITIONS_CHANGED for a directory that holds no
/// healthy Order of the Package.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_batch_order_draft(
    batch: &SpielordersBatch,
    directory: PathBytes<'_>,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let package = batch.data.package();
        let directory = directory.as_path();
        let find = |directory: &std::path::Path| {
            package
                .healthy_orders
                .iter()
                .enumerate()
                .find(|(_, order)| order.paths.directory.absolute_path == directory)
        };
        // The directory as the caller spelled it, and only then as the filesystem resolves
        // it, so a relative or linked spelling still names its Order.
        let found = find(directory).or_else(|| {
            crate::ports::FileSystem::canonicalize(&Session::uncancellable(), directory)
                .ok()
                .and_then(|resolved| find(&resolved))
        });
        let result = match found {
            Some((position, order)) => Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_ORDER,
                Origin::EditOrder {
                    directory: order.paths.directory.absolute_path.clone(),
                    original: Box::new(order.document.clone()),
                },
                order_facts(order, package.vendor_for_order(position)),
                order
                    .document
                    .items
                    .iter()
                    .map(|item| item_facts(item, None))
                    .collect(),
            )),
            None => {
                Entry::failures(
                    &Failure::PreconditionsChanged {
                        location: crate::readme::located_in_package(
                            &package.context.root.absolute_path,
                            directory.to_path_buf(),
                        ),
                    },
                    Subject::Order,
                    entries,
                );
                Err(crate::interface::SPIELORDERS_RESULT_REFUSED)
            }
        };
        deliver(result, draft)
    })
}

/// A draft of the Vendor `selector` (SPIELORDERS_VENDOR_SELECTOR_NAME or _ID) and `text`
/// select as `batch` holds it, saved by adding it to the batch.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_batch_vendor_draft(
    batch: &SpielordersBatch,
    selector: SpielordersConstant,
    text: Text<'_>,
    draft: &mut Option<Box<SpielordersDraft>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let package = batch.data.package();
        let result = (|| {
            let text = text.to_str().ok_or(SPIELORDERS_RESULT_BAD_ARGUMENT)?;
            let vendor = match selector {
                SPIELORDERS_VENDOR_SELECTOR_NAME => {
                    crate::vendors::vendor_by_name(text, &package.vendors)
                }
                SPIELORDERS_VENDOR_SELECTOR_ID => package
                    .vendors
                    .iter()
                    .find(|vendor| vendor.vendor_id.as_str() == text),
                _ => return Err(SPIELORDERS_RESULT_BAD_ARGUMENT),
            };
            let Some(vendor) = vendor else {
                let failure = match selector {
                    SPIELORDERS_VENDOR_SELECTOR_ID => Failure::UnknownVendorId {
                        vendor_id: text.to_owned(),
                    },
                    _ => Failure::UnknownVendor {
                        name: text.to_owned(),
                        scope: crate::vendors::vendor_aggregate(package),
                    },
                };
                Entry::failures(&failure, Subject::Vendor, entries);
                return Err(crate::interface::SPIELORDERS_RESULT_REFUSED);
            };
            Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_VENDOR,
                Origin::EditVendor {
                    root: package.context.root.absolute_path.clone(),
                    original: Box::new(vendor.clone()),
                },
                vendor_facts(vendor),
                vendor.contacts.iter().map(contact_facts).collect(),
            ))
        })();
        deliver(result, draft)
    })
}

/// A draft of a new Vendor in `batch`, saved by adding it to the batch, where it is given
/// its Vendor ID so that later changes in the batch can name it.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_batch_new_vendor_draft(
    batch: &SpielordersBatch,
    draft: &mut Option<Box<SpielordersDraft>>,
) -> SpielordersResult {
    fallible(None, |_| {
        let package = batch.data.package();
        deliver(
            Ok(SpielordersDraft::new(
                SPIELORDERS_DRAFT_KIND_VENDOR,
                Origin::NewVendor {
                    root: package.context.root.absolute_path.clone(),
                },
                vec![(
                    SPIELORDERS_VENDOR_FIELD_NAME,
                    new_vendor_name(&package.vendors),
                )],
                Vec::new(),
            )),
            draft,
        )
    })
}

/// Plans saving `draft` against the Package as `batch` holds it and, where every rule
/// holds, adds it to the batch; a refused draft leaves the batch unchanged, its entries
/// located by field and row. A batch holds edits to existing Orders and Vendors and new
/// Vendors; any other draft refuses SPIELORDERS_FAILURE_NOT_BATCHABLE.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_batch_add(
    batch: &mut SpielordersBatch,
    draft: &SpielordersDraft,
    token: u64,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::new(token);
        if session.is_cancelled() {
            return crate::interface::SPIELORDERS_RESULT_CANCELLED;
        }
        draft.guarded(
            SPIELORDERS_RESULT_BAD_ARGUMENT,
            SPIELORDERS_RESULT_INTERNAL,
            |data| {
                let Some(change) = data.change() else {
                    return SPIELORDERS_RESULT_BAD_ARGUMENT;
                };
                let added = Answer::of(|| {
                    batch.data.add(
                        &change,
                        &|| session.today(),
                        &|| session.new_uuid(),
                        &session,
                    )
                });
                // Adding is an atomic in-memory commit. Once it succeeds, a later
                // cancellation must not report that the accepted change was omitted.
                if added.result.is_ok() {
                    crate::interface::SPIELORDERS_RESULT_OK
                } else {
                    settle(added, &session, data.subject(), entries).map_or_else(
                        |result| result,
                        |()| crate::interface::SPIELORDERS_RESULT_OK,
                    )
                }
            },
        )
    })
}

/// How many changes `batch` holds.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_batch_count(batch: &SpielordersBatch) -> i64 {
    quietly(-1, || i64::try_from(batch.data.len()).unwrap_or(-1))
}

/// Plans committing every change `batch` holds as one guarded write of the records they
/// touched, refused at commit when any of those records changed since the batch's
/// Package was opened. spielorders_plan_order_count is how many Orders it rewrites.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_batch_plan(
    batch: &SpielordersBatch,
    token: u64,
    plan: &mut Option<Box<SpielordersPlan>>,
    outcome: Option<&mut Option<Box<SpielordersEntries>>>,
) -> SpielordersResult {
    fallible(outcome, |entries| {
        let session = Session::new(token);
        let root = batch.data.package().context.root.absolute_path.clone();
        let result = settle(
            crate::change::plan_batch(&batch.data),
            &session,
            Subject::Other,
            entries,
        )
        .map(|value| SpielordersPlan::of(value, root, Subject::Other, |_| Vec::new()));
        deliver(result, plan)
    })
}

/// Frees `batch`. Freeing NULL does nothing.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_batch_free(batch: Option<Box<SpielordersBatch>>) {
    quietly((), || drop(batch));
}
