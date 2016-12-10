//! This module holds our search system.
//!
//! It implements the full-text capabilities of our Clouseau crate, as well as
//! adding some Turtl-specific indexing to the Clouseau sqlite connection.
//!
//! Note that this module only returns note IDs when returning search results.

use ::rusqlite::types::{ToSql, sqlite3_stmt};
use ::libc::c_int;

use ::clouseau::Clouseau;

use ::error::TResult;
use ::models::model;
use ::models::note::Note;
use ::models::file::File;

/// A query builder
#[derive(Debug)]
pub struct Query {
    /// Full-text search query
    text: Option<String>,
    /// Boards (OR)
    boards: Vec<String>,
    /// Tags (AND)
    tags: Vec<String>,
    /// Tags we've excluded
    exclude_tags: Vec<String>,
    /// Search on type
    type_: Option<String>,
    /// Search on whether we have a file or not
    has_file: Option<bool>,
    /// Search by color
    color: Option<i32>,
    /// What we're sorting on
    sort: String,
    /// What sort direction
    sort_direction: String,
    /// Result page
    page: i32,
    /// Results per page
    per_page: i32,
}

impl Query {
    /// Create a new search query builder
    pub fn new() -> Query {
        Query {
            text: None,
            boards: Vec::new(),
            tags: Vec::new(),
            exclude_tags: Vec::new(),
            type_: None,
            has_file: None,
            color: None,
            sort: String::from("id"),
            sort_direction: String::from("desc"),
            page: 1,
            per_page: 100,
        }
    }

    /// Set the full-text search query
    pub fn text<'a>(&'a mut self, text: String) -> &'a mut Self {
        self.text = Some(text);
        self
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
    pub fn exclude_tags<'a>(&'a mut self, exclude_tags: Vec<String>) -> &'a mut Self {
        self.exclude_tags = exclude_tags;
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
    pub fn sort<'a>(&'a mut self, sort: String) -> &'a mut Self {
        self.sort = sort;
        self
    }

    /// Our sort direction
    pub fn sort_direction<'a>(&'a mut self, sort_direction: String) -> &'a mut Self {
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

/// Makes generating SQL statements somewhat painless by implementing rusqlite's
/// ToSql for some primitive types (wrapped in one enum).
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
        let id_mod = match model::id_timestamp(&id) {
            Ok(x) => x,
            Err(_) => 99999999,
        };
        let has_file = get_field!(note, has_file, false);
        let mod_ = get_field!(note, mod_, id_mod) as i64;
        let type_ = get_field!(note, type_, String::from("text"));
        let color = get_field!(note, color, 0);
        self.idx.conn.execute("INSERT INTO notes (id, has_file, mod, type, color) VALUES (?, ?, ?, ?, ?)", &[&id, &has_file, &mod_, &type_, &color])?;

        let boards = get_field!(note, boards, Vec::new());
        let tags = get_field!(note, tags, Vec::new());
        for board in boards {
            self.idx.conn.execute("INSERT INTO notes_boards (note_id, board_id) VALUES (?, ?)", &[&id, &board])?;
        }
        for tag in tags {
            self.idx.conn.execute("INSERT INTO notes_tags (note_id, tag) VALUES (?, ?)", &[&id, &tag])?;
        }
        let note_body = [
            get_field!(note, title, String::from("")),
            get_field!(note, text, String::from("")),
            get_field!(note, tags, Vec::new()).as_slice().join(" "),
            get_field!(note, url, String::from("")),
            {
                let fakefile = File::new();
                let file = get_field!(note, file, &fakefile);
                get_field!(file, name, String::from(""))
            },
        ].join(" ");
        self.idx.index(&id, &note_body)?;
        Ok(())
    }

    /// Unindex a note
    pub fn unindex_note(&self, note: &Note) -> TResult<()> {
        model_getter!(get_field, "Search.unindex_note()");
        let id = get_field!(note, id);
        self.idx.conn.execute("DELETE FROM notes WHERE id = ?", &[&id])?;
        self.idx.conn.execute("DELETE FROM notes_boards where note_id = ?", &[&id])?;
        self.idx.conn.execute("DELETE FROM notes_tags where note_id = ?", &[&id])?;
        self.idx.unindex(&id)?;
        Ok(())
    }

    /// Unindex/reindex a note
    pub fn reindex_note(&self, note: &Note) -> TResult<()> {
        self.unindex_note(note)?;
        self.index_note(note)
    }

    /// Search for notes. Returns the note IDs only. Loading them from the db
    /// and decrypting are up to you...OR YOUR MOM.
    ///
    /// NOTE: This function uses a lot of vector concatenation and joining to
    /// build our queries. It's probably pretty slow and inefficient. On top of
    /// that, it makes extensive use of SQL's `intersect` to grab results from a
    /// bunch of separate queries. There may be a more efficient way to do this,
    /// however since this is all in-memory anyway, it's probably fine.
    pub fn find(&self, query: &Query) -> TResult<Vec<String>> {
        let mut queries: Vec<String> = Vec::new();
        let mut exclude_queries: Vec<String> = Vec::new();
        let mut qry_vals: Vec<SearchVal> = Vec::new();

        // this one is kind of weird. we basically do
        //   SELECT id FROM notes WHERE id IN (id1, id2)
        // there's probably a much better way, but this is easiest for now
        if query.text.is_some() {
            let ft_note_ids = self.idx.find(query.text.as_ref().unwrap())?;
            let mut ft_qry: Vec<&str> = Vec::with_capacity(ft_note_ids.len() + 2);
            ft_qry.push("SELECT id FROM notes WHERE id IN (");
            for id in &ft_note_ids {
                if id == &ft_note_ids[ft_note_ids.len() - 1] {
                    ft_qry.push("?");
                } else {
                    ft_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(id.clone()));
            }
            ft_qry.push(")");
            queries.push(ft_qry.as_slice().join(""));
        }

        if query.boards.len() > 0 {
            let mut board_qry: Vec<&str> = Vec::with_capacity(query.boards.len() + 2);
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
            let mut tag_qry: Vec<&str> = Vec::with_capacity(query.tags.len() + 2);
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

        if query.exclude_tags.len() > 0 {
            let mut excluded_tag_qry: Vec<&str> = Vec::with_capacity(query.exclude_tags.len() + 2);
            excluded_tag_qry.push("SELECT note_id FROM notes_tags WHERE tag IN (");
            for excluded_tag in &query.exclude_tags {
                if excluded_tag == &query.exclude_tags[query.exclude_tags.len() - 1] {
                    excluded_tag_qry.push("?");
                } else {
                    excluded_tag_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(excluded_tag.clone()));
            }
            excluded_tag_qry.push(") GROUP BY note_id HAVING COUNT(*) = ?");
            qry_vals.push(SearchVal::Int(query.exclude_tags.len() as i32));
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
        let orderby = format!(" ORDER BY {} {}", query.sort, query.sort_direction);
        let pagination = format!(" LIMIT {} OFFSET {}", query.per_page, (query.page - 1) * query.per_page);
        let final_query = (filter_query + &orderby) + &pagination;

        let mut prepared_qry = self.idx.conn.prepare(final_query.as_str())?;
        let mut values: Vec<&ToSql> = Vec::with_capacity(qry_vals.len());
        for val in &qry_vals {
            let ts: &ToSql = val;
            values.push(ts);
        }
        let rows = prepared_qry.query_map(values.as_slice(), |row| row.get(0))?;
        let mut note_ids = Vec::new();
        for id in rows { note_ids.push(id?); }
        Ok(note_ids)
    }

    /// Grab note tags out of the index, sorted by frequency of use (desc).
    /// Takes a vec of board_ids to limit the search to, but if passed a zero
    /// length vec, will just pull out all tags.
    pub fn tags_by_frequency(&self, boards: &Vec<String>, limit: i32) -> TResult<Vec<(String, i32)>> {
        let mut tag_qry: Vec<&str> = Vec::with_capacity(boards.len() + 4);
        let mut qry_vals: Vec<SearchVal> = Vec::new();
        tag_qry.push("SELECT tag, count(tag) AS tag_count FROM notes_tags ");
        if boards.len() > 0 {
            tag_qry.push("WHERE note_id IN (SELECT note_id FROM notes_boards WHERE board_id IN (");
            for board in boards {
                if board == &boards[boards.len() - 1] {
                    tag_qry.push("?");
                } else {
                    tag_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(board.clone()));
            }
            tag_qry.push(")) ");
        }
        tag_qry.push("GROUP BY tag ORDER BY tag_count DESC, tag ASC LIMIT ?");
        qry_vals.push(SearchVal::Int(limit));

        let final_query = tag_qry.as_slice().join("");
        let mut prepared_qry = self.idx.conn.prepare(final_query.as_str())?;
        let mut values: Vec<&ToSql> = Vec::with_capacity(qry_vals.len());
        for val in &qry_vals {
            let ts: &ToSql = val;
            values.push(ts);
        }
        let rows = prepared_qry.query_map(values.as_slice(), |row| (row.get("tag"), row.get("tag_count")))?;
        let mut tags = Vec::new();
        for entry in rows {
            let val = entry?;
            tags.push((val.0, val.1));
        }
        Ok(tags)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use ::jedi;
    use ::models::note::Note;

    #[test]
    fn loads_search() {
        // seems stupic, but let's make sure our queries work
        Search::new().unwrap();
    }

    #[test]
    fn index_unindex_filter() {
        let search = Search::new().unwrap();

        let note1: Note = jedi::parse(&String::from(r#"{"id":"1111","type":"text","title":"CNN News Report","text":"Wow, terrible. Just terrible. So many bad things are happening. Are you safe? We just don't know! You could die tomorrow! You're probably only watching this because you're at the airport...here are some images of airplanes crashing! Oh, by the way, where are your children?! They are probably being molested by dozens and dozens of pedophiles right now, inside of a building that is going to be attacked by terrorists! What can you do about it? NOTHING! Do you have breast cancer??? Stay tuned to learn more!","tags":["news","cnn","airplanes","terrorists","breasts"],"boards":["6969","1212"]}"#)).unwrap();
        let note2: Note = jedi::parse(&String::from(r#"{"id":"2222","type":"link","title":"Fox News Report","text":"Aren't liberals stupid??! I mean, right? Did you know...Obama is BLACK! We have to stop him! We need to block EVERYTHING he does, even if we agreed with it a few years ago, because he's BLACK. How dare him?! Also, we should, like, give tax breaks to corporations. They deserve a break, people. Stop being so greedy and give the corporations a break. COMMUNISTS.","tags":["news","fox","fair","balanced","corporations"],"url":"https://fox.com/news/daily-report"}"#)).unwrap();
        let note3: Note = jedi::parse(&String::from(r#"{"id":"3333","type":"text","title":"Buzzfeed","text":"Other drivers hate him!!1 Find out why! Are you wasting thousands of dollars on insurance?! This one weird tax loophole has the IRS furious! New report shows the color of your eyes determines the SIZE OF YOUR PENIS AND/OR BREASTS <Ad for colored contacts>!!","tags":["buzzfeed","weird","simple","trick","breasts"],"boards":["6969"]}"#)).unwrap();
        let note4: Note = jedi::parse(&String::from(r#"{"id":"4444","type":"text","title":"Libertarian news","text":"TAXES ARE THEFT. AYN RAND WAS RIGHT ABOUT EVERYTHING EXCEPT FOR ALL THE THINGS SHE WAS WRONG ABOUT WHICH WAS EVERYTHING. WE DON'T NEED REGULATIONS BECAUSE THE MARKET IS MORAL. NET NEUTRALITY IS COMMUNISM. DO YOU ENJOY USING UR COMPUTER?! ...WELL IT WAS BUILD WITH THE FREE MARKET, COMMUNIST. TAXES ARE SLAVERY. PROPERTY RIGHTS.","tags":["liberatrians","taxes","property rights","socialism"],"boards":["1212","8989"]}"#)).unwrap();
        let note5: Note = jedi::parse(&String::from(r#"{"id":"5555","type":"text","title":"Any News Any Time","text":"Peaceful protests happened today amid the news of Trump being elected. In other news, VIOLENT RIOTS broke out because a bunch of native americans are angry about some stupid pipeline. They are so violent, these natives. They don't care about their lands being polluted by corrupt government or corporate forces, they just like blowing shit up. They just cannot find it in their icy hearts to leave the poor pipeline corporations alone. JUST LEAVE THEM ALONE. THE PIPELINE WON'T POLLUTE! CORPORATIONS DON'T LIE SO LEAVE THEM ALONE!!","tags":["pipeline","protests","riots","corporations"],"boards":["8989","6969"]}"#)).unwrap();

        search.index_note(&note1).unwrap();
        search.index_note(&note2).unwrap();
        search.index_note(&note3).unwrap();
        search.index_note(&note4).unwrap();
        search.index_note(&note5).unwrap();

        // board search
        let mut query = Query::new();
        query.boards(vec![String::from("6969")]);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555", "3333", "1111"]);

        // board search w/ paging
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .page(2)
            .per_page(1);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["3333"]);

        // combine boards/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .tags(vec![String::from("terrorists")]);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // combining boards/tags/sort
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"(penis OR "icy hearts")"#))
            .sort(String::from("id"));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555", "3333"]);

        // combining boards/tags/sort/desc
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"(penis OR "icy hearts")"#))
            .sort(String::from("id"))
            .sort_direction(String::from("asc"));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["3333", "5555"]);

        // combining boards/text/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"(penis OR "icy hearts")"#))
            .tags(vec![String::from("riots")]);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555"]);

        // do tags show up in a text search? they should
        let mut query = Query::new();
        query.text(String::from(r#"socialism"#));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["4444"]);

        // excluded tags!
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .exclude_tags(vec![String::from("weird")])
            .sort(String::from("mod"))
            .sort_direction(String::from("asc"));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111", "5555"]);

        // tag frequency search
        let tags = search.tags_by_frequency(&Vec::new(), 9999).unwrap();
        assert_eq!(
            tags,
            vec![
                (String::from("breasts"), 2),
                (String::from("corporations"), 2),
                (String::from("news"), 2),
                (String::from("airplanes"), 1),
                (String::from("balanced"), 1),
                (String::from("buzzfeed"), 1),
                (String::from("cnn"), 1),
                (String::from("fair"), 1),
                (String::from("fox"), 1),
                (String::from("liberatrians"), 1),
                (String::from("pipeline"), 1),
                (String::from("property rights"), 1),
                (String::from("protests"), 1),
                (String::from("riots"), 1),
                (String::from("simple"), 1),
                (String::from("socialism"), 1),
                (String::from("taxes"), 1),
                (String::from("terrorists"), 1),
                (String::from("trick"), 1),
                (String::from("weird"), 1),
            ]
        );
        let tags = search.tags_by_frequency(&vec![String::from("6969")], 9999).unwrap();
        assert_eq!(
            tags,
            vec![
                (String::from("breasts"), 2),
                (String::from("airplanes"), 1),
                (String::from("buzzfeed"), 1),
                (String::from("cnn"), 1),
                (String::from("corporations"), 1),
                (String::from("news"), 1),
                (String::from("pipeline"), 1),
                (String::from("protests"), 1),
                (String::from("riots"), 1),
                (String::from("simple"), 1),
                (String::from("terrorists"), 1),
                (String::from("trick"), 1),
                (String::from("weird"), 1),
            ]
        );
        let tags = search.tags_by_frequency(&vec![String::from("6969"), String::from("1212")], 9999).unwrap();
        assert_eq!(
            tags,
            vec![
                (String::from("breasts"), 2),
                (String::from("airplanes"), 1),
                (String::from("buzzfeed"), 1),
                (String::from("cnn"), 1),
                (String::from("corporations"), 1),
                (String::from("liberatrians"), 1),
                (String::from("news"), 1),
                (String::from("pipeline"), 1),
                (String::from("property rights"), 1),
                (String::from("protests"), 1),
                (String::from("riots"), 1),
                (String::from("simple"), 1),
                (String::from("socialism"), 1),
                (String::from("taxes"), 1),
                (String::from("terrorists"), 1),
                (String::from("trick"), 1),
                (String::from("weird"), 1),
            ]
        );

        // ---------------------------------------------------------------------
        // reindex note 3
        // ---------------------------------------------------------------------
        let note3: Note = jedi::parse(&String::from(r#"{"id":"3333","type":"text","title":"Buzzfeed","text":"BREAKING NEWS Auto insurance companies HATE this one simple trick! Are you a good person? Here are ten questions you can ask yourself to find out. You won't believe number eight!!!!","tags":["buzzfeed","quiz","insurance"],"boards":["6969"]}"#)).unwrap();
        search.reindex_note(&note3).unwrap();

        // combining boards/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"(penis OR "icy hearts")"#));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555"]);

        // combining boards/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"one simple trick"#));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["3333"]);

        // combining boards/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"simple tricks"#));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);

        // ---------------------------------------------------------------------
        // remove some notes, rerun
        // ---------------------------------------------------------------------
        search.unindex_note(&note3).unwrap();
        search.unindex_note(&note5).unwrap();

        // board search
        let mut query = Query::new();
        query.boards(vec![String::from("6969")]);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // combine boards/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .tags(vec![String::from("terrorists")]);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // combining boards/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"(penis OR "icy hearts")"#));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);

        // combining boards/text/tags
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .text(String::from(r#"(penis OR "icy hearts")"#))
            .tags(vec![String::from("riots")]);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);

        // do tags show up in a text search? they should
        let mut query = Query::new();
        query.text(String::from(r#"socialism"#));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["4444"]);

        // excluded tags!
        let mut query = Query::new();
        query
            .boards(vec![String::from("6969")])
            .exclude_tags(vec![String::from("weird")]);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // type
        let mut query = Query::new();
        query.ty(String::from("link"));
        let notes = search.find(&query).unwrap();
        assert_eq!(notes, vec!["2222"]);

        // color
        let mut query = Query::new();
        query
            .color(3)
            .has_file(true);
        let notes = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);
    }
}

