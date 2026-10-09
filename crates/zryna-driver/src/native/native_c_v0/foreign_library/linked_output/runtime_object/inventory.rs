//! Match strong definitions to the retained ABI; validate every local and undefined symbol.

use super::{
    Diagnostic, IMPORTS, error,
    tables::{self, Tables},
};
use std::collections::BTreeSet;

pub(super) struct Symbol<'a> {
    pub(super) name: &'a str,
    pub(super) kind: u8,
    pub(super) section: usize,
    pub(super) value: u64,
}

fn identifier(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().enumerate().all(|(i, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || (i > 0 && byte.is_ascii_digit())
        })
}

pub(super) fn check<'a>(
    table: &Tables<'a>,
    expected: &BTreeSet<&str>,
) -> Result<Vec<Symbol<'a>>, Diagnostic> {
    let mut result = Vec::new();
    let mut defined = BTreeSet::new();
    let mut imported = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut names = BTreeSet::new();
    for (ordinal, row) in table.symbols.chunks_exact(24).enumerate() {
        let name = tables::string(table.strings, tables::number(row, 0, 4)?)?;
        let binding = row[4] >> 4;
        let kind = row[4] & 15;
        let section = usize::try_from(tables::number(row, 6, 2)?).map_err(|_| error())?;
        let value = tables::number(row, 8, 8)?;
        let size = tables::number(row, 16, 8)?;
        if ordinal != 0 {
            if binding > 1 || row[5] != 0 || (binding == 0) != (ordinal < table.first_global) {
                return Err(error());
            }
            match (binding, kind, section) {
                (1, 0, 0) => {
                    if value != 0 || size != 0 || !IMPORTS.contains(&name) || !imported.insert(name)
                    {
                        return Err(error());
                    }
                }
                (0, 4, 0xfff1) => {
                    if name.is_empty() || value != 0 || size != 0 {
                        return Err(error());
                    }
                }
                (0, 3, index) if index != 0 => {
                    if !name.is_empty()
                        || value != 0
                        || size != 0
                        || !table.sections.get(index).is_some_and(|s| {
                            matches!(s.name, ".text" | ".data" | ".bss" | ".rodata")
                        })
                    {
                        return Err(error());
                    }
                }
                (binding, 1 | 2, index) if index != 0 => {
                    // OWNERSHIP_RUNTIME_V1 fixes zryna_rt_o1_ as the reserved ABI namespace.
                    let target = table.sections.get(index).ok_or_else(error)?;
                    let end = value.checked_add(size).ok_or_else(error)?;
                    if !identifier(name)
                        || !names.insert(name)
                        || size == 0
                        || end > target.size
                        || (kind == 2 && index != table.text_index)
                        || (kind == 1 && !matches!(target.name, ".data" | ".bss" | ".rodata"))
                        || (binding == 1
                            && (kind != 2 || !expected.contains(name) || !defined.insert(name)))
                        || (binding == 0 && name.starts_with("zryna_rt_o1_"))
                    {
                        return Err(error());
                    }
                    ranges.push((index, value, end));
                }
                _ => return Err(error()),
            }
        }
        result.push(Symbol { name, kind, section, value });
    }
    ranges.sort_unstable();
    if &defined != expected
        || imported != BTreeSet::from(IMPORTS)
        || ranges.windows(2).any(|pair| pair[0].0 == pair[1].0 && pair[0].2 > pair[1].1)
    {
        return Err(error());
    }
    Ok(result)
}
