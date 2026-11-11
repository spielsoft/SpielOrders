//! The library itself: its version and the name tables of every constant.

use crate::interface::{
    SpielordersConstant, Text,
    call::quietly,
    constants::{FAMILIES, NAMES},
    entries::EMPTY,
};

/// This library's version, as text.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_library_version() -> Text<'static> {
    const VERSION: &std::ffi::CStr = match std::ffi::CStr::from_bytes_with_nul(
        concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes(),
    ) {
        Ok(version) => version,
        Err(_) => c"",
    };
    Text::new(VERSION)
}

/// A suggested agent request after provisioning. Borrowed static UTF-8; never freed.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_library_agent_setup_prompt() -> Text<'static> {
    Text::new(c"Read .agents/skills/spielorders/SKILL.md. Add a concise pointer to it in AGENTS.md, preserving all existing instructions. Use the installed SpielOrders CLI for managed data.")
}

/// The name of `value`: the snake_case spelling of any `SPIELORDERS_*` constant,
/// such as "damaged_package" for SPIELORDERS_FAILURE_DAMAGED_PACKAGE, or "" for a value
/// that is no constant.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_constant_name(value: SpielordersConstant) -> Text<'static> {
    quietly(Text::new(EMPTY), || {
        NAMES
            .iter()
            .find(|(constant, _)| *constant == value)
            .map(|(_, name)| *name)
            .or_else(|| {
                FAMILIES
                    .iter()
                    .find(|(family, _, _)| *family == value)
                    .map(|(_, name, _)| *name)
            })
            .map_or(Text::new(EMPTY), Text::new)
    })
}

/// The member of `family` (a `SPIELORDERS_FAMILY_*` constant) that `name` spells, or -1
/// when the family has no such member or `name` is not UTF-8.
#[unsafe(no_mangle)]
pub extern "C" fn spielorders_constant_from_name(
    family: SpielordersConstant,
    name: Text<'_>,
) -> SpielordersConstant {
    quietly(SpielordersConstant::NONE, || {
        name.to_str()
            .and_then(|name| member(family, name))
            .unwrap_or(SpielordersConstant::NONE)
    })
}

/// The range a family's members occupy.
pub(crate) fn range(family: SpielordersConstant) -> Option<std::ops::Range<i32>> {
    FAMILIES
        .iter()
        .find(|(constant, _, _)| *constant == family)
        .map(|(_, _, base)| *base..if *base == 0 { 100 } else { base + 1000 })
}

/// Whether `value` is a member of `family`.
pub(crate) fn is_member(family: SpielordersConstant, value: SpielordersConstant) -> bool {
    range(family).is_some_and(|range| range.contains(&value.0))
        && NAMES.iter().any(|(constant, _)| *constant == value)
}

/// The member of `family` that `name` spells.
pub(crate) fn member(family: SpielordersConstant, name: &str) -> Option<SpielordersConstant> {
    let range = range(family)?;
    NAMES
        .iter()
        .find(|(constant, spelled)| {
            range.contains(&constant.0) && spelled.to_bytes() == name.as_bytes()
        })
        .map(|(constant, _)| *constant)
}

/// The name of `value`, as Rust text.
pub(crate) fn name(value: SpielordersConstant) -> Option<&'static str> {
    NAMES
        .iter()
        .find(|(constant, _)| *constant == value)
        .and_then(|(_, name)| name.to_str().ok())
}
