//! This module holds our search system.
//!
//! It implements the full-text capabilities of our Clouseau crate, as well as
//! adding some Turtl-specific indexing to the Clouseau sqlite connection.
//!
//! Note that this module only returns note IDs when returning search results.

use ::rusqlite::types::ToSql;

use ::clouseau::Clouseau;
use ::dumpy::SearchVal;

use ::error::{TResult, TError};
use ::models::model;
use ::models::note::Note;
use ::models::file::File;

/// A query builder
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Query {
    pub text: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    pub space_id: String,
    #[serde(default)]
    pub boards: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub exclude_tags: Vec<String>,
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub url: Option<String>,
    pub has_file: Option<bool>,
    pub color: Option<i32>,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub sort_direction: String,
    #[serde(default)]
    pub page: i32,
    #[serde(default)]
    pub per_page: i32,
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
        idx.conn.execute("CREATE TABLE IF NOT EXISTS notes (id VARCHAR(64) PRIMARY KEY, space_id VARCHAR(96), board_id VARCHAR(96), has_file BOOL, created INTEGER, mod INTEGER, type VARCHAR(32), color INTEGER, url VARCHAR(256))", &[])?;
        idx.conn.execute("CREATE TABLE IF NOT EXISTS notes_tags (id ROWID, note_id VARCHAR(64), tag VARCHAR(128))", &[])?;
        Ok(Search {
            idx: idx,
        })
    }

    /// Index a note
    pub fn index_note(&mut self, note: &Note) -> TResult<()> {
        model_getter!(get_field, "Search.index_note()");
        let id = get_field!(note, id);
        let id_mod = match model::id_timestamp(&id) {
            Ok(x) => x,
            Err(_) => 99999999,
        };
        let space_id = note.space_id.clone();
        if space_id == "" {
            return TErr!(TError::MissingField(format!("Note {} missing `space_id`", id)));
        }
        let board_id = get_field!(note, board_id, String::from(""));
        let board_id = if board_id == "" { None } else { Some(board_id) };
        let has_file = note.has_file;
        let mod_ = note.mod_;
        let type_ = get_field!(note, type_, String::from("text"));
        let color = get_field!(note, color, 0);
        self.idx.conn.execute(
            "INSERT INTO notes (id, space_id, board_id, has_file, created, mod, type, color, url) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[&id, &space_id, &board_id, &has_file, &id_mod, &mod_, &type_, &color, &note.url]
        )?;

        let tags = get_field!(note, tags, Vec::new());
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
    pub fn unindex_note(&mut self, note: &Note) -> TResult<()> {
        model_getter!(get_field, "Search.unindex_note()");
        let id = get_field!(note, id);
        self.idx.conn.execute("DELETE FROM notes WHERE id = ?", &[&id])?;
        self.idx.conn.execute("DELETE FROM notes_tags where note_id = ?", &[&id])?;
        self.idx.unindex(&id)?;
        Ok(())
    }

    /// Unindex/reindex a note
    pub fn reindex_note(&mut self, note: &Note) -> TResult<()> {
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
    pub fn find(&self, query: &Query) -> TResult<(Vec<String>, i32)> {
        let mut queries: Vec<String> = Vec::new();
        let mut exclude_queries: Vec<String> = Vec::new();
        let mut qry_vals: Vec<SearchVal> = Vec::new();

        let mut space_qry: Vec<&str> = Vec::with_capacity(1);
        space_qry.push("SELECT id FROM notes WHERE space_id = ?");
        qry_vals.push(SearchVal::String(query.space_id.clone()));
        queries.push(space_qry.as_slice().join(""));

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

        if query.notes.len() > 0 {
            let mut note_qry: Vec<&str> = Vec::with_capacity(query.notes.len() + 2);
            note_qry.push("SELECT id FROM notes WHERE id IN (");
            for note_id in &query.notes {
                if note_id == &query.notes[query.notes.len() - 1] {
                    note_qry.push("?");
                } else {
                    note_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(note_id.clone()));
            }
            note_qry.push(")");
            queries.push(note_qry.as_slice().join(""));
        }

        if query.boards.len() > 0 {
            let mut board_qry: Vec<&str> = Vec::with_capacity(query.boards.len() + 2);
            board_qry.push("SELECT id FROM notes WHERE board_id IN (");
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

        if query.url.is_some() {
            queries.push(String::from("SELECT id FROM notes WHERE url = ?"));
            qry_vals.push(SearchVal::String(query.url.as_ref().unwrap().clone()));
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
        let mut sort = query.sort.clone();
        let mut sort_dir = query.sort_direction.clone();
        let mut page = query.page;
        let mut per_page = query.per_page;
        if sort == "" { sort = String::from("id"); }
        if sort_dir == "" { sort_dir = String::from("desc"); }
        if page < 1 { page = 1; }
        if per_page < 1 { per_page = 50; }

        let orderby = format!(" ORDER BY {} {}", sort, sort_dir);
        let pagination = format!(" LIMIT {} OFFSET {}", per_page, (page - 1) * per_page);
        let final_query = (filter_query.clone() + &orderby) + &pagination;
        let total_query = format!("SELECT COUNT(search.id) AS total FROM ({}) AS search", filter_query);

        let mut prepared_qry = self.idx.conn.prepare(final_query.as_str())?;
        let mut values: Vec<&ToSql> = Vec::with_capacity(qry_vals.len());
        for val in &qry_vals {
            let ts: &ToSql = val;
            values.push(ts);
        }
        let rows = prepared_qry.query_map(values.as_slice(), |row| row.get(0))?;
        let mut note_ids = Vec::new();
        for id in rows { note_ids.push(id?); }

        let total = self.idx.conn.query_row(total_query.as_str(), values.as_slice(), |row| {
            row.get("total")
        })?;

        debug!("Search.find() -- grabbed {} notes ({} total)", note_ids.len(), total);
        Ok((note_ids, total))
    }

    /// Given a query object, find the tags that match it. This disregards page
    /// and per_page, since we want a list of all tags that match that result.
    pub fn find_tags(&self, query: &Query) -> TResult<Vec<(String, i32)>> {
        let mut query = query.clone();
        query.page = 1;
        query.per_page = 99999;
        let (note_ids, _total) = self.find(&query)?;
        self.tags_by_notes(&note_ids)
    }

    /// Given a set of note ids, grab the tags for hose notes and their
    /// frequency.
    pub fn tags_by_notes(&self, note_ids: &Vec<String>) -> TResult<Vec<(String, i32)>> {
        if note_ids.len() == 0 {
            return Ok(Vec::new());
        }
        let mut tag_qry: Vec<&str> = Vec::with_capacity(note_ids.len() + 4);
        let mut qry_vals: Vec<SearchVal> = Vec::new();
        tag_qry.push("SELECT tag, count(tag) AS tag_count FROM notes_tags WHERE note_id IN (");
        if note_ids.len() > 0 {
            for note_id in note_ids {
                if note_id == &note_ids[note_ids.len() - 1] {
                    tag_qry.push("?");
                } else {
                    tag_qry.push("?,");
                }
                qry_vals.push(SearchVal::String(note_id.clone()));
            }
            tag_qry.push(") ");
        }
        tag_qry.push("GROUP BY tag ORDER BY tag_count DESC, tag ASC");

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

impl Drop for Search {
    fn drop(&mut self) {
        match self.idx.close() {
            Ok(_) => {},
            Err(e) => {
                warn!("Search.drop() -- problem closing search index, oh well... {}", e);
            }
        }
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
        fn parserrr(json: &str) -> Query {
            jedi::parse(&json.replacen("{", r#"{"space_id":"4455","#, 1)).unwrap()
        }

        let mut search = Search::new().unwrap();

        let note1: Note = jedi::parse(&String::from(r#"{"id":"1111","space_id":"4455","user_id":69,"type":"text","title":"CNN News Report","text":"Wow, terrible. Just terrible. So many bad things are happening. Are you safe? We just don't know! You could die tomorrow! You're probably only watching this because you're at the airport...here are some images of airplanes crashing! Oh, by the way, where are your children?! They are probably being molested by dozens and dozens of pedophiles right now, inside of a building that is going to be attacked by terrorists! What can you do about it? NOTHING! Do you have breast cancer??? Stay tuned to learn more!","tags":["news","cnn","airplanes","terrorists","breasts"],"board_id":"6969"}"#)).unwrap();
        let note2: Note = jedi::parse(&String::from(r#"{"id":"2222","space_id":"4455","user_id":69,"type":"link","title":"Fox News Report","text":"Aren't liberals stupid??! I mean, right? Did you know...Obama is BLACK! We have to stop him! We need to block EVERYTHING he does, even if we agreed with it a few years ago, because he's BLACK. How dare him?! Also, we should, like, give tax breaks to corporations. They deserve a break, people. Stop being so greedy and give the corporations a break. COMMUNISTS.","tags":["news","fox","fair","balanced","corporations"],"url":"https://fox.com/news/daily-report"}"#)).unwrap();
        let note3: Note = jedi::parse(&String::from(r#"{"id":"3333","space_id":"4455","user_id":69,"type":"text","title":"Buzzfeed","text":"Other drivers hate him!!1 Find out why! Are you wasting thousands of dollars on insurance?! This one weird tax loophole has the IRS furious! New report shows the color of your eyes determines the SIZE OF YOUR PENIS AND/OR BREASTS <Ad for colored contacts>!!","tags":["buzzfeed","weird","simple","trick","breasts"],"board_id":"6969"}"#)).unwrap();
        let note4: Note = jedi::parse(&String::from(r#"{"id":"4444","space_id":"4455","user_id":69,"type":"text","title":"Libertarian news","text":"TAXES ARE THEFT. AYN RAND WAS RIGHT ABOUT EVERYTHING EXCEPT FOR ALL THE THINGS SHE WAS WRONG ABOUT WHICH WAS EVERYTHING. WE DON'T NEED REGULATIONS BECAUSE THE MARKET IS MORAL. NET NEUTRALITY IS COMMUNISM. DO YOU ENJOY USING UR COMPUTER?! ...WELL IT WAS BUILD WITH THE FREE MARKET, COMMUNIST. TAXES ARE SLAVERY. PROPERTY RIGHTS.","tags":["liberatrians","taxes","property rights","socialism"],"board_id":"1212"}"#)).unwrap();
        let note5: Note = jedi::parse(&String::from(r#"{"id":"5555","space_id":"4455","user_id":69,"type":"text","title":"Any News Any Time","text":"Peaceful protests happened today amid the news of Trump being elected. In other news, VIOLENT RIOTS broke out because a bunch of native americans are angry about some stupid pipeline. They are so violent, these natives. They don't care about their lands being polluted by corrupt government or corporate forces, they just like blowing shit up. They just cannot find it in their icy hearts to leave the poor pipeline corporations alone. JUST LEAVE THEM ALONE. THE PIPELINE WON'T POLLUTE! CORPORATIONS DON'T LIE SO LEAVE THEM ALONE!!","tags":["pipeline","protests","riots","corporations"],"board_id":"6969"}"#)).unwrap();
        // NOTE: this is space_id "0000", so won't turn up in searches!!!!11
        let note6: Note = jedi::parse(&String::from(r#"{"id":"5556","space_id":"0000","user_id":69,"type":"text","title":"Any News Any Time","text":"Peaceful protests happened today amid the news of Trump being elected. In other news, VIOLENT RIOTS broke out because a bunch of native americans are angry about some stupid pipeline. They are so violent, these natives. They don't care about their lands being polluted by corrupt government or corporate forces, they just like blowing shit up. They just cannot find it in their icy hearts to leave the poor pipeline corporations alone. JUST LEAVE THEM ALONE. THE PIPELINE WON'T POLLUTE! CORPORATIONS DON'T LIE SO LEAVE THEM ALONE!!","tags":["pipeline","protests","riots","corporations"],"board_id":"6969"}"#)).unwrap();

        search.index_note(&note1).unwrap();
        search.index_note(&note2).unwrap();
        search.index_note(&note3).unwrap();
        search.index_note(&note4).unwrap();
        search.index_note(&note5).unwrap();
        search.index_note(&note6).unwrap();

        // search by note ids
        let query = parserrr(r#"{"notes":["1111","4444","6969loljkomg"]}"#);
        let (notes, total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["4444", "1111"]);
        assert_eq!(total, 2);

        // board search
        let query = parserrr(r#"{"boards":["6969"]}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555", "3333", "1111"]);

        // board search w/ paging
        let query = parserrr(r#"{"boards":["6969"],"page":2,"per_page":1}"#);
        let (notes, total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["3333"]);
        assert_eq!(total, 3);

        // combine boards/tags
        let query = parserrr(r#"{"boards":["6969"],"tags":["terrorists"]}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // combining boards/tags/sort
        let query = parserrr(r#"{"boards":["6969"],"text":"(penis OR \"icy hearts\")","sort":"id"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555", "3333"]);

        // combining boards/tags/sort/desc
        let query = parserrr(r#"{"boards":["6969"],"text":"(penis OR \"icy hearts\")","sort":"id","sort_direction":"asc"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["3333", "5555"]);

        // combining boards/text/tags
        let query = parserrr(r#"{"boards":["6969"],"text":"(penis OR \"icy hearts\")","tags":["riots"]}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555"]);

        // do tags show up in a text search? they should
        let query = parserrr(r#"{"text":"socialism"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["4444"]);

        // excluded tags!
        let query = parserrr(r#"{"boards":["6969"],"exclude_tags":["weird"],"sort":"mod","sort_direction":"asc"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111", "5555"]);

        // tag frequency search
        let qry: Query = jedi::from_val(json!({
            "space_id": "4455",
            "boards": [],
        })).unwrap();
        let tags = search.find_tags(&qry).unwrap();
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
        let qry: Query = jedi::from_val(json!({
            "space_id": "4455",
            "boards": ["6969"],
        })).unwrap();
        let tags = search.find_tags(&qry).unwrap();
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
        let qry: Query = jedi::from_val(json!({
            "space_id": "4455",
            "boards": ["6969", "1212"],
        })).unwrap();
        let tags = search.find_tags(&qry).unwrap();
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
        let note3: Note = jedi::parse(&String::from(r#"{"id":"3333","space_id":"4455","user_id":69,"type":"text","title":"Buzzfeed","text":"BREAKING NEWS Auto insurance companies HATE this one simple trick! Are you a good person? Here are ten questions you can ask yourself to find out. You won't believe number eight!!!!","tags":["buzzfeed","quiz","insurance"],"board_id":"6969"}"#)).unwrap();
        search.reindex_note(&note3).unwrap();

        // combining boards/tags
        let query = parserrr(r#"{"boards":["6969"],"text":"(penis OR \"icy hearts\")"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["5555"]);

        // combining boards/tags
        let query = parserrr(r#"{"boards":["6969"],"text":"one simple trick"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["3333"]);

        // combining boards/tags
        let query = parserrr(r#"{"boards":["6969"],"text":"simple tricks"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);

        // ---------------------------------------------------------------------
        // remove some notes, rerun
        // ---------------------------------------------------------------------
        search.unindex_note(&note3).unwrap();
        search.unindex_note(&note5).unwrap();

        // board search
        let query = parserrr(r#"{"boards":["6969"]}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // combine boards/tags
        let query = parserrr(r#"{"boards":["6969"],"tags":["terrorists"]}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // combining boards/tags
        let query = parserrr(r#"{"boards":["6969"],"text":"(penis OR \"icy hearts\")"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);

        // combining boards/text/tags
        let query = parserrr(r#"{"boards":["6969"],"text":"(penis OR \"icy hearts\")","tags":["riots"]}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);

        // do tags show up in a text search? they should
        let query = parserrr(r#"{"text":"socialism"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["4444"]);

        // excluded tags!
        let query = parserrr(r#"{"boards":["6969"],"exclude_tags":["weird"]}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["1111"]);

        // type
        let query = parserrr(r#"{"type":"link"}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes, vec!["2222"]);

        // color
        let query = parserrr(r#"{"color":3,"has_file":true}"#);
        let (notes, _total) = search.find(&query).unwrap();
        assert_eq!(notes.len(), 0);
    }
}

