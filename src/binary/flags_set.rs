use std::borrow::Cow;
use std::collections::HashMap;

use smallvec::SmallVec;
use watto::StringTable;

use super::{TestAnalyticsError, TestAnalyticsErrorKind};

type FlagsMap = HashMap<SmallVec<u32, 4>, u32>;

#[derive(Debug, Default, Clone)]
pub struct FlagsSet<'table> {
    pub(crate) map: FlagsMap,
    pub(crate) table: Cow<'table, [u32]>,
}

impl<'table> FlagsSet<'table> {
    /// Inserts the given `flags`, using the given `string_table` as string buffer.
    pub fn insert(&mut self, string_table: &mut StringTable, flags: &[&str]) -> u32 {
        let mut flags: SmallVec<&str, 4> = flags.into();
        flags.sort();
        flags.dedup();

        let flags = flags
            .iter()
            .map(|flag| string_table.insert(flag) as u32)
            .collect();

        *self.map.entry(flags).or_insert_with_key(|flags| {
            let offset = self.table.len();
            let table = self.table.to_mut();
            table.push(flags.len() as u32);
            table.extend(flags);
            offset as u32
        })
    }

    /// Loads the flags `table`, validating that all flag names are valid `string_table` references.
    pub fn load(string_bytes: &[u8], table: &'table [u32]) -> Result<Self, TestAnalyticsError> {
        let mut map = FlagsMap::default();
        let mut offset = 0;
        let mut rest = table;

        while let Some((len, new_rest)) = rest.split_first() {
            let (flags, new_rest) = new_rest
                .split_at_checked(*len as usize)
                .ok_or(TestAnalyticsErrorKind::InvalidTables)?;

            // validate all the flags
            for flag in flags {
                StringTable::read(string_bytes, *flag as usize)
                    .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;
            }

            map.insert(flags.into(), offset);

            offset += 1 + len;
            rest = new_rest;
        }

        Ok(Self {
            map,
            table: table.into(),
        })
    }

    pub fn iter<'slf, 'strings>(
        &'slf self,
        string_bytes: &'strings [u8],
    ) -> impl ExactSizeIterator<Item = Result<(u32, SmallVec<&'strings str, 4>), TestAnalyticsError>>
           + use<'slf, 'strings> {
        self.map.iter().map(|(flags_offsets, offset)| {
            let mut flags: SmallVec<&'strings str, 4> =
                SmallVec::with_capacity(flags_offsets.len());
            for flag in flags_offsets {
                let flag = StringTable::read(string_bytes, *flag as usize)
                    .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;
                flags.push(flag);
            }

            Ok((*offset, flags))
        })
    }

    pub fn resolve<'strings>(
        &self,
        string_bytes: &'strings [u8],
        offset: u32,
    ) -> Result<SmallVec<&'strings str, 4>, TestAnalyticsError> {
        let len = self
            .table
            .get(offset as usize)
            .ok_or(TestAnalyticsErrorKind::InvalidFlagSetReference)?;
        let len = *len as usize;
        let range_start = offset as usize + 1;
        let range = range_start..range_start + len;
        let flags_raw = self
            .table
            .get(range)
            .ok_or(TestAnalyticsErrorKind::InvalidFlagSetReference)?;

        let mut flags = SmallVec::with_capacity(len);
        for flag in flags_raw {
            let string = StringTable::read(string_bytes, *flag as usize)
                .map_err(|_| TestAnalyticsErrorKind::InvalidStringReference)?;
            flags.push(string);
        }
        Ok(flags)
    }

    pub fn to_owned(&self) -> FlagsSet<'static> {
        FlagsSet {
            map: self.map.clone(),
            table: self.table.clone().into_owned().into(),
        }
    }
}
