use crate::{
    result::{ReedlineError, ReedlineErrorVariants},
    History, HistoryItem, HistoryItemId, Result, SearchDirection, SearchFilter, SearchQuery,
};

/// A history that wraps another history, and does not forward values beginning with `exclusion_prefix` (if present).
/// If an item is filtered, it is stored in memory and able to be retrieved, until the next item is inserted.
#[derive(Debug)]
pub(crate) struct HistoryFilter<T> {
    pub wrapped: T,
    // if Some(s), buffers starting with `s` will not be saved in history
    exclusion_prefix: Option<String>,
    excluded_last_item: Option<HistoryItem>,
}

// It should always be possible to add 1 to a HistoryItemId without overflowing.
const LAST_ITEM_ID: i64 = i64::MAX - 1;
impl<T> HistoryFilter<T> {
    /// Create a new filter, wrapping `wrapped`. No filters are enabled by default.
    pub fn new(wrapped: T) -> Self {
        Self {
            wrapped,
            exclusion_prefix: None,
            excluded_last_item: None,
        }
    }

    /// Change or remove prefix used to filter items. Existing entries are not modified.
    pub fn set_exclusion_prefix(&mut self, exclusion_prefix: Option<String>) {
        self.exclusion_prefix = exclusion_prefix;
    }
}

impl<T: History> History for HistoryFilter<T> {
    fn save(&mut self, mut h: HistoryItem) -> Result<HistoryItem> {
        if let Some(exclusion_prefix) = &self.exclusion_prefix {
            if h.command_line.starts_with(exclusion_prefix) {
                h.id = Some(HistoryItemId::new(LAST_ITEM_ID));
                self.excluded_last_item = Some(h.clone());
                return Ok(h);
            }
        }
        self.excluded_last_item = None;
        self.wrapped.save(h)
    }

    fn load(&self, id: HistoryItemId) -> Result<HistoryItem> {
        if id.0 == LAST_ITEM_ID {
            self.excluded_last_item.clone().ok_or(ReedlineError(
                ReedlineErrorVariants::OtherHistoryError("Item does not exist"),
            ))
        } else {
            self.wrapped.load(id)
        }
    }

    // Count doesn't include filtered item.
    fn count(&self, query: SearchQuery) -> Result<i64> {
        self.wrapped.count(query)
    }

    fn search(&self, query: SearchQuery) -> Result<Vec<HistoryItem>> {
        let append = if let Some(excluded_item) = &self.excluded_last_item {
            if matches!(
                query,
                SearchQuery {
                    direction: _,
                    start_time: None,
                    end_time: None,
                    start_id: _,
                    end_id: _,
                    limit: _,
                    filter: SearchFilter {
                        command_line: None,
                        not_command_line: _,
                        hostname: _,
                        cwd_exact: _,
                        cwd_prefix: _,
                        exit_successful: _
                    }
                }
            ) {
                match (query.start_id.map(|x| x.0), query.end_id.map(|x| x.0)) {
                    (None, None)
                        if query.limit == Some(1)
                            && query.direction == SearchDirection::Backward =>
                    {
                        return Ok(vec![excluded_item.clone()]);
                    }
                    (None, Some(LAST_ITEM_ID)) | (None, None) => Some(excluded_item.clone()),
                    (Some(start), Some(LAST_ITEM_ID)) | (Some(start), None)
                        if start < LAST_ITEM_ID && query.direction == SearchDirection::Forward =>
                    {
                        Some(excluded_item.clone())
                    }
                    (_, _) => None,
                }
            } else {
                None
            }
        } else {
            None
        };

        let direction = query.direction;
        let limit = query.limit;
        let mut res = self.wrapped.search(query)?;
        if let Some(append) = append {
            if limit
                .map(|limit| i64::try_from(res.len()).unwrap() < limit)
                .unwrap_or(true)
            {
                if matches!(direction, SearchDirection::Forward) {
                    res.push(append);
                } else {
                    res.insert(0, append);
                }
            }
        }
        Ok(res)
    }

    fn update(
        &mut self,
        id: HistoryItemId,
        updater: &dyn Fn(HistoryItem) -> HistoryItem,
    ) -> Result<()> {
        if id.0 == LAST_ITEM_ID {
            if let Some(excluded_item) = &mut self.excluded_last_item {
                *excluded_item = updater(excluded_item.clone());
                Ok(())
            } else {
                Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                    "Could not find item",
                )))
            }
        } else {
            self.wrapped.update(id, updater)
        }
    }

    fn clear(&mut self) -> Result<()> {
        self.excluded_last_item = None;
        self.wrapped.clear()
    }

    fn delete(&mut self, h: HistoryItemId) -> Result<()> {
        if h.0 == LAST_ITEM_ID {
            if self.excluded_last_item.is_some() {
                self.excluded_last_item = None;
                Ok(())
            } else {
                Err(ReedlineError(ReedlineErrorVariants::OtherHistoryError(
                    "Could not find item",
                )))
            }
        } else {
            self.wrapped.delete(h)
        }
    }

    fn sync(&mut self) -> std::io::Result<()> {
        self.wrapped.sync()
    }
}
