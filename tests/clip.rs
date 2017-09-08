include!("./_lib.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip() {
        let handle = init();
        let res = dispatch_ass(json!([
            "clip",
            "https://turtlapp.com/",
            []
        ]));
        assert!(jedi::get_opt::<String>(&["title"], &res).is_some());
        assert!(jedi::get_opt::<String>(&["description"], &res).is_some());
        assert!(jedi::get_opt::<String>(&["image_url"], &res).is_none());
        end(handle);
    }
}


