//! This module holds our search system.
//!
//! It implements the full-text capabilities of our Clouseau crate, as well as
//! adding some Turtl-specific indexing to the Clouseau sqlite connection.
//!
//! Note that this module only returns note IDs when returning search results.

use ::rusqlite::types::{ToSql, Null, sqlite3_stmt};
use ::rusqlite::types::Value as SqlValue;
use ::libc::c_int;

use ::clouseau::Clouseau;

use ::error::{TResult, TError};
use ::models::note::Note;

/// Used to specify what field we're sorting our results on
pub enum Sort {
    /// Sort by create date
    Created,
    /// Sort by mod date
    Mod,
}

/// Defines our sort direction
pub enum SortDirection {
    Asc,
    Desc,
}

/// A query builder
pub struct Query {
    /// Boards (OR)
    boards: Vec<String>,
    /// Tags (AND)
    tags: Vec<String>,
    /// Tags we've excluded
    excluded_tags: Vec<String>,
    /// Search on type
    type_: Option<String>,
    /// Search on whether we have a file or not
    has_file: Option<bool>,
    /// Search by color
    color: Option<i32>,
    /// What we're sorting on
    sort: Sort,
    /// What sort direction
    sort_direction: SortDirection,
    /// Result page
    page: i32,
    /// Results per page
    per_page: i32,
}

impl Query {
    /// Create a new search query builder
    pub fn new() -> Query {
        Query {
            boards: Vec::new(),
            tags: Vec::new(),
            excluded_tags: Vec::new(),
            type_: None,
            has_file: None,
            color: None,
            sort: Sort::Created,
            sort_direction: SortDirection::Desc,
            page: 1,
            per_page: 100,
        }
    }

    /// Set boards in the search
    pub fn boards<'a>(&'a mut self, boards: Vec<String>) -> &'a mut Self {
        self.boards = boards;
        self
    }

    /// Set tags into the search
    pub fn tags<'a>(&'a mut self, tags: Vec<String>) -> &'a mut Self {
        self.tags = tags;
        self
    }

    /// Set excluded tags into the search
    pub fn excluded_tags<'a>(&'a mut self, excluded_tags: Vec<String>) -> &'a mut Self {
        self.excluded_tags = excluded_tags;
        self
    }

    /// Search by type
    pub fn ty<'a>(&'a mut self, type_: String) -> &'a mut Self {
        self.type_ = Some(type_);
        self
    }

    /// Set has_file into the search
    pub fn has_file<'a>(&'a mut self, has_file: bool) -> &'a mut Self {
        self.has_file = Some(has_file);
        self
    }

    /// Set color into the search
    pub fn color<'a>(&'a mut self, color: i32) -> &'a mut Self {
        self.color = Some(color);
        self
    }

    /// How to sort our results
    pub fn sort<'a>(&'a mut self, sort: Sort) -> &'a mut Self {
        self.sort = sort;
        self
    }

    /// Our sort direction
    pub fn sort_direction<'a>(&'a mut self, sort_direction: SortDirection) -> &'a mut Self {
        self.sort_direction = sort_direction;
        self
    }

    /// Set our result page
    pub fn page<'a>(&'a mut self, page: i32) -> &'a mut Self {
        self.page = page;
        self
    }

    /// Set our results per page
    pub fn per_page<'a>(&'a mut self, per_page: i32) -> &'a mut Self {
        self.per_page = per_page;
        self
    }
}

/// Holds the state for our search
pub struct Search {
    /// Our main index, driven by Clouseau. Mainly for full-text search, but is
    /// used for other indexed searches as well.
    idx: Clouseau,
}

unsafe impl Send for Search {}
unsafe impl Sync for Search {}

impl Search {
    /// Create a new Search object
    pub fn new() -> TResult<Search> {
        let idx = Clouseau::new()?;
        idx.conn.execute("CREATE TABLE IF NOT EXISTS notes (id VARCHAR(64) PRIMARY KEY, has_file BOOL, mod INTEGER, type VARCHAR(32), color INTEGER)", &[])?;
        idx.conn.execute("CREATE TABLE IF NOT EXISTS notes_boards (id ROWID, note_id VARCHAR(64), board_id VARCHAR(64))", &[])?;
        idx.conn.execute("CREATE TABLE IF NOT EXISTS notes_tags (id ROWID, note_id VARCHAR(64), tag VARCHAR(128))", &[])?;
        Ok(Search {
            idx: idx,
        })
    }

    /// Index a note
    pub fn index_note(&self, note: &Note) -> TResult<()> {
        model_getter!(get_field, "Search.index_note()");
        let id = get_field!(note, id);
        let has_file = get_field!(note, has_file);
        let mod_ = get_field!(note, mod_) as i64;
        let type_ = get_field!(note, type_);
        let color = get_field!(note, color);
        self.idx.conn.execute("INSERT INTO notes (id, has_file, mod, type, color) VALUES (?, ?, ?, ?, ?)", &[&id, &has_file, &mod_, &type_, &color])?;

        let boards = get_field!(note, boards);
        let tags = get_field!(note, tags);
        for board in boards {
            self.idx.conn.execute("INSERT INTO notes_boards (note_id, board_id) VALUES (?, ?)", &[&id, &board])?;
        }
        for tag in tags {
            self.idx.conn.execute("INSERT INTO notes_tags (note_id, tag) VALUES (?, ?)", &[&id, &tag])?;
        }
        Ok(())
    }

    /// Unindex a note
    pub fn unindex_note(&self, note: &Note) -> TResult<()> {
        model_getter!(get_field, "Search.unindex_note()");
        let id = get_field!(note, id);
        self.idx.conn.execute("DELETE FROM notes WHERE id = ?", &[&id])?;
        self.idx.conn.execute("DELETE FROM notes_boards where note_id = ?", &[&id])?;
        self.idx.conn.execute("DELETE FROM notes_tags where note_id = ?", &[&id])?;
        Ok(())
    }

    /// Unindex/reindex a note
    pub fn reindex_note(&self, note: &Note) -> TResult<()> {
        self.unindex_note(note)?;
        self.index_note(note)
    }

    /// Search for notes. Returns the note IDs only. Loading them from the db
    /// and decrypting are up to you...OR YOUR MOM.
    pub fn find(&self, query: &Query) -> TResult<Vec<String>> {
        enum SearchVal {
            Bool(bool),
            String(String),
            Int(i32),
        }
        impl ToSql for SearchVal {
            unsafe fn bind_parameter(&self, stmt: *mut sqlite3_stmt, col: c_int) -> c_int {
                match *self {
                    SearchVal::Bool(ref x) => x.bind_parameter(stmt, col),
                    SearchVal::Int(ref x) => x.bind_parameter(stmt, col),
                    SearchVal::String(ref x) => x.bind_parameter(stmt, col),
                }
            }
        }

        let mut queries: Vec<String> = Vec::new();
        let mut exclude_queries: Vec<String> = Vec::new();
        let mut qry_vals: Vec<SearchVal> = Vec::new();

        if query.boards.len() > 0 {
            let mut board_qry: Vec<&str> = Vec::new();
            board_qry.push("SELECT note_id FROM notes_boards WHERE board_id IN (");
            for board in &query.boards {
                if board == &query.boards[query.boards.len() - 1] {
                    board_qry.push("?");
                } else {
                    board_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(board.clone()));
            }
            board_qry.push(")");
            queries.push(board_qry.as_slice().join(""));
        }

        if query.tags.len() > 0 {
            let mut tag_qry: Vec<&str> = Vec::new();
            tag_qry.push("SELECT note_id FROM notes_tags WHERE tag IN (");
            for tag in &query.tags {
                if tag == &query.tags[query.tags.len() - 1] {
                    tag_qry.push("?");
                } else {
                    tag_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(tag.clone()));
            }
            tag_qry.push(") GROUP BY note_id HAVING COUNT(*) = ?");
            qry_vals.push(SearchVal::Int(query.tags.len() as i32));
            queries.push(tag_qry.as_slice().join(""));
        }

        if query.excluded_tags.len() > 0 {
            let mut excluded_tag_qry: Vec<&str> = Vec::new();
            excluded_tag_qry.push("SELECT note_id FROM notes_tags WHERE tag IN (");
            for excluded_tag in &query.excluded_tags {
                if excluded_tag == &query.excluded_tags[query.excluded_tags.len() - 1] {
                    excluded_tag_qry.push("?");
                } else {
                    excluded_tag_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(excluded_tag.clone()));
            }
            excluded_tag_qry.push(") GROUP BY note_id HAVING COUNT(*) = ?");
            qry_vals.push(SearchVal::Int(query.excluded_tags.len() as i32));
            exclude_queries.push(excluded_tag_qry.as_slice().join(""));
        }

        if query.type_.is_some() {
            queries.push(String::from("SELECT id FROM notes WHERE type = ?"));
            qry_vals.push(SearchVal::String(query.type_.as_ref().unwrap().clone()));
        }

        if query.has_file.is_some() {
            queries.push(String::from("SELECT id FROM notes WHERE has_file = ?"));
            qry_vals.push(SearchVal::Bool(query.has_file.as_ref().unwrap().clone()));
        }

        if query.color.is_some() {
            queries.push(String::from("SELECT id FROM notes WHERE color = ?"));
            qry_vals.push(SearchVal::Int(query.color.as_ref().unwrap().clone()));
        }

        let filter_query = if queries.len() > 0 && exclude_queries.len() > 0 {
            let include = queries.as_slice().join(" intersect ");
            let exclude = exclude_queries.as_slice().join(" union ");
            format!("SELECT id FROM notes WHERE id IN ({}) AND id NOT IN ({})", include, exclude)
        } else if queries.len() > 0 {
            let include = queries.as_slice().join(" intersect ");
            format!("SELECT id FROM notes WHERE id IN ({})", include)
        } else if exclude_queries.len() > 0 {
            let exclude = exclude_queries.as_slice().join(" union ");
            format!("SELECT id FROM notes WHERE id NOT IN ({})", exclude)
        } else {
            String::from("SELECT id FROM notes")
        };
        let orderby = format!(" ORDER BY {} {}", match query.sort {
            Sort::Created => "created",
            Sort::Mod => "mod",
        }, match query.sort_direction {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        });

        let pagination = format!(" LIMIT {} OFFSET {}", query.page, (query.page - 1) * query.per_page);
        let final_query = (filter_query + &orderby) + &pagination;

        let mut prepared_qry = self.idx.conn.prepare(final_query.as_str())?;
        let mut values: Vec<&ToSql> = Vec::with_capacity(qry_vals.len());
        for val in &qry_vals {
            let ts: &ToSql = val;
            values.push(ts);
        }
        let rows = prepared_qry.query_map(values.as_slice(), |row| row.get(0))?;
        let mut note_ids = Vec::new();
        for id in rows {
            note_ids.push(id?);
        }
        Ok(note_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_search() {
        Search::new().unwrap();
    }
}

