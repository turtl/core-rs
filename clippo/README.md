# Clippo

A really crappy/lazy web clipper. Give it a URL and it attempts to grab a few
pieces of information: a title, a description, and the most relevant image. It
currently has two methods of doing this:

- **HTML parsing/selectors** - Loads and parses the URL's HTML and lets you run
selectors against the "DOM" object. Really great for pages that pre-generate
their HTML.
- **Parsing of JSON in HTML** - Basically built specifically for Youtube. Takes
a regex used to find a JSON block in the page returned by the URL, then parses
the JSON and allows selecting bits of information out of it using paths.

Examples of both can be found in the `parsers.yaml` file, which includes some
default parsers (can be expanded on as needed without recompiling).

Note that this library was built specifically for web clipping for the Turtl
core-rs project, meaning it is privacy-centric: it will not pass the URL you
give it off to some untrusted server somewhere for processing. This limits its
capabilities (HTML5/SPAs are somewhat un-clippable). Perhaps in the future it
can support a server component, but it would have to be opt-in.

