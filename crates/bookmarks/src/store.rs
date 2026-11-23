//! The Bookmarks store: `<id>.md` files with the front matter `id`,
//! `address`, `title`, `site`, `cover`, `excerpt`, `author`, `published` and
//! `saved` and a list of `tags`; the body is the Article, if the Bookmark has
//! one. The Cover and the Article's images are files beside them, each
//! `<random-id>.<ext>` (Bookmarks ADR-0002, ADR-0029).

use crate::address::normalise;
use crate::extract::Details;
use crate::images;
use crate::state::{Anchor, Highlight, Markup, PageComment, Place, Rgb, State};
use crate::sync::UPDATED;
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::files::{DiskFiles, Files};
use pkm_base::front_matter::{Document, Value};
use pkm_base::images::{extension, is_image_file};
use pkm_base::item_id::ItemId;
use pkm_base::markup::MarkupChanges;
use pkm_base::random::Entropy;
use pkm_base::tag;
use pkm_base::tag_store::TagStore;
use pkm_search::{Needle, Snippet, start_of};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

const ADDRESS: &str = "address";
const TITLE: &str = "title";
const SITE: &str = "site";
const COVER: &str = "cover";
const EXCERPT: &str = "excerpt";
const AUTHOR: &str = "author";
const PUBLISHED: &str = "published";
const SAVED: &str = "saved";
const TAGS: &str = "tags";
/// Beside the Bookmarks, hidden: one state file per Bookmark.
const STATE_FOLDER: &str = ".state";

/// A Bookmark and its details. Empty where there is none, except `cover`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub id: String,
    /// Normalised: no two Bookmarks share one.
    pub address: String,
    pub title: String,
    pub site: String,
    /// The file name of the Cover beside the Bookmark.
    pub cover: Option<String>,
    pub excerpt: String,
    pub author: String,
    pub published: String,
    /// When it was Saved, as UTC ISO 8601.
    pub saved: String,
    /// Whether the Bookmark has an Article.
    pub has_article: bool,
    /// Valid Tags, in the order they were added.
    pub tags: Vec<String>,
    /// Where the Article was left; none if it was never read, or there is no Article.
    pub progress: Option<ArticleProgress>,
}

/// Where in a Bookmark's Article the user left off, and when (Bookmarks ADR-0002).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArticleProgress {
    /// UTF-16 offset into the Article's stored Markdown (`read_article`), never past its end.
    pub offset: u64,
    /// Last Read: when the Reader last rested, in milliseconds since the Unix epoch.
    pub last_read: u64,
    /// How much of the Article lies before `offset`, 0 to 100, rounded down.
    pub percent: u8,
}

/// What Saving gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub bookmark: Bookmark,
    /// The Address was Saved before, so this is that Bookmark, unchanged.
    pub existing: bool,
}

/// A Bookmark that Search found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub bookmark: Bookmark,
    /// Around the first match in the Article, else in the Site or Address;
    /// the start of the Article (the Excerpt without one), with no match
    /// marked, where only the Title matched.
    pub snippet: Snippet,
}

/// The Bookmarks in one folder (`Bookmarks` in the app's storage).
#[derive(Debug)]
pub struct Bookmarks {
    files: Arc<dyn Files>,
    root: PathBuf,
    tags: TagStore,
}

impl Bookmarks {
    /// The folder's name, in the app's storage and in the cloud.
    pub const FOLDER: &'static str = "Bookmarks";

    /// Opens the Bookmarks in `root`, creating the folder if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self::open_in(Arc::new(DiskFiles), root))
    }

    /// The Bookmarks in `root` on any `files`.
    pub fn open_in(files: Arc<dyn Files>, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            tags: TagStore::beside(files.clone(), &root),
            files,
            root,
        }
    }

    /// Saves the page at `address` with its `details` (see `extract`) unless
    /// its Address is Saved already, which gives that Bookmark unchanged.
    /// `title` and `tags` are the user's: a blank `title` leaves it to the
    /// page's, and invalid Tags are left out. `article` is the Article's
    /// Markdown (see `read_article`) if the Bookmark is to have one; blank is
    /// none. `images` holds the bytes downloaded for the Cover and the
    /// addresses the Article names: those are kept beside the Bookmark and the
    /// Article points at the files. An image that wasn't downloaded or is no
    /// PNG, JPEG, GIF or WebP is left out, so that a broken image never costs
    /// the Bookmark. `InvalidInput` for an `address` that is none (see `normalise`).
    pub fn save(
        &self,
        address: &str,
        details: &Details,
        title: &str,
        tags: &[String],
        article: Option<&str>,
        images: &HashMap<String, Vec<u8>>,
    ) -> io::Result<Saved> {
        let address = normalise(address).ok_or(io::ErrorKind::InvalidInput)?;
        if let Some(bookmark) = self.list()?.into_iter().find(|b| b.address == address) {
            return Ok(Saved {
                bookmark,
                existing: true,
            });
        }
        let mut entropy = Entropy::from_system();
        let id = self.unused_id(&mut entropy);
        let cover = match details.cover.as_ref().and_then(|cover| images.get(cover)) {
            Some(bytes) => self.keep_image(&mut entropy, bytes)?,
            None => None,
        };
        let title = [one_line(title), one_line(&details.title)]
            .into_iter()
            .find(|t| !t.is_empty())
            .unwrap_or_else(|| address.clone());
        let mut file = Document::default();
        file.set_scalar("id", id.as_str());
        file.set_scalar(ADDRESS, &address);
        file.set_scalar(TITLE, title);
        for (key, value) in [
            (SITE, one_line(&details.site)),
            (COVER, cover.unwrap_or_default()),
            (EXCERPT, one_line(&details.excerpt)),
            (AUTHOR, one_line(&details.author)),
            (PUBLISHED, one_line(&details.published)),
        ] {
            if !value.is_empty() {
                file.set_scalar(key, value);
            }
        }
        file.set_scalar(SAVED, iso8601(now_ms()));
        let mut kept_tags: Vec<String> = Vec::new();
        for tag in tags.iter().filter(|t| tag::is_valid(t)) {
            if !kept_tags.contains(tag) {
                kept_tags.push(tag.clone());
            }
        }
        set_tags(&mut file, kept_tags.clone());
        if let Some(article) = article {
            file.body = self.keep_article(&mut entropy, article, images)?;
        }
        self.files.write(&self.path(&id), &file.render())?;
        self.tags.put_on(kept_tags)?;
        Ok(Saved {
            bookmark: self.bookmark_of(&file, &id),
            existing: false,
        })
    }

    /// Every Bookmark, newest Saved first. Files that are no Bookmark are skipped.
    pub fn list(&self) -> io::Result<Vec<Bookmark>> {
        let mut all: Vec<_> = (self.all_files()?.iter())
            .map(|(id, file)| self.bookmark_of(file, id))
            .collect();
        all.sort_by(|a, b| b.saved.cmp(&a.saved).then_with(|| a.id.cmp(&b.id)));
        Ok(all)
    }

    /// The Bookmarks whose Title, Site, Address or Article has `query`,
    /// ignoring case and accents, anywhere in a word and in the text as
    /// written. Those matching in the Title come first, then the others, each
    /// newest Saved first. A blank query finds nothing. Reads every Bookmark
    /// on each call.
    pub fn search(&self, query: &str) -> io::Result<Vec<Found>> {
        let Some(needle) = Needle::new(query) else {
            return Ok(Vec::new());
        };
        let mut hits = Vec::new();
        for (id, file) in self.all_files()? {
            let bookmark = self.bookmark_of(&file, &id);
            let title_matched = needle.find(&bookmark.title).is_some();
            let snippet = (needle.find(&file.body))
                .or_else(|| needle.find(&bookmark.site))
                .or_else(|| needle.find(&bookmark.address));
            if snippet.is_none() && !title_matched {
                continue;
            }
            let snippet = snippet.unwrap_or_else(|| {
                start_of(if bookmark.has_article {
                    &file.body
                } else {
                    &bookmark.excerpt
                })
            });
            hits.push((title_matched, Found { bookmark, snippet }));
        }
        hits.sort_by(|(a_title, a), (b_title, b)| {
            (b_title.cmp(a_title))
                .then_with(|| b.bookmark.saved.cmp(&a.bookmark.saved))
                .then_with(|| a.bookmark.id.cmp(&b.bookmark.id))
        });
        Ok(hits.into_iter().map(|(_, found)| found).collect())
    }

    /// Article Search: where `query` is in the Article of the Bookmark `id`,
    /// matched as Search does. Each range is UTF-16 offsets into the stored
    /// Markdown (`read_article`), in order; none for a blank query.
    /// `NotFound` if there is no such Bookmark or it has no Article.
    pub fn search_article(&self, id: &str, query: &str) -> io::Result<Vec<Range<usize>>> {
        let article = self.read_article(id)?;
        Ok(Needle::new(query).map_or_else(Vec::new, |needle| needle.find_all(&article)))
    }

    /// The Bookmark `id`; `NotFound` if there is none.
    pub fn read(&self, id: &str) -> io::Result<Bookmark> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let file = self.read_file(&id).ok_or(io::ErrorKind::NotFound)?;
        Ok(self.bookmark_of(&file, &id))
    }

    /// The bytes of the Cover of the Bookmark `id`; `NotFound` if there is
    /// no such Bookmark or it has no Cover.
    pub fn read_cover(&self, id: &str) -> io::Result<Vec<u8>> {
        let name = self.read(id)?.cover.ok_or(io::ErrorKind::NotFound)?;
        self.files.read_bytes(&self.root.join(name))
    }

    /// The Article of the Bookmark `id`; `NotFound` if there is no such
    /// Bookmark or it has no Article.
    pub fn read_article(&self, id: &str) -> io::Result<String> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let file = self.read_file(&id).ok_or(io::ErrorKind::NotFound)?;
        Some(file.body)
            .filter(|b| !b.trim().is_empty())
            .ok_or(io::ErrorKind::NotFound.into())
    }

    /// The bytes of the image `name`, as the Article of the Bookmark `id`
    /// names it; `NotFound` if there is no such Bookmark, or the Article
    /// doesn't use it, or it is gone.
    pub fn read_image(&self, id: &str, name: &str) -> io::Result<Vec<u8>> {
        let bookmark = self.read(id)?;
        let used = bookmark.cover.as_deref() == Some(name)
            || self.images_of(&bookmark).iter().any(|n| n == name);
        if !used {
            return Err(io::ErrorKind::NotFound.into());
        }
        self.files.read_bytes(&self.root.join(name))
    }

    /// Every Bookmark carrying exactly `tag`, newest Saved first.
    pub fn list_tagged(&self, tag: &str) -> io::Result<Vec<Bookmark>> {
        let mut all = self.list()?;
        all.retain(|b| b.tags.iter().any(|t| t == tag));
        Ok(all)
    }

    /// The Tags some Bookmark carries, by Relevance: what the list can be
    /// narrowed to.
    pub fn tags_in_use(&self) -> io::Result<Vec<String>> {
        let tags: BTreeSet<String> = self.list()?.into_iter().flat_map(|b| b.tags).collect();
        Ok(self.tags.read().ordered(tags))
    }

    /// Renames the Bookmark `id` (one line, like Saving does); a blank
    /// `title` leaves it as it is. `NotFound` if there is no such Bookmark.
    pub fn rename(&self, id: &str, title: &str) -> io::Result<Bookmark> {
        let title = one_line(title);
        self.modify(id, |file| {
            if !title.is_empty() {
                file.set_scalar(TITLE, title);
            }
        })
    }

    /// Puts `tag` on the Bookmark `id`, unless it has it; a new Tag joins the
    /// Tags file. `InvalidInput` for an invalid Tag, `NotFound` if there is no
    /// such Bookmark.
    pub fn add_tag(&self, id: &str, tag: &str) -> io::Result<Bookmark> {
        if !tag::is_valid(tag) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let mut put_on = false;
        let bookmark = self.modify(id, |file| {
            let mut tags = tags_of(file);
            if !tags.iter().any(|t| t == tag) {
                tags.push(tag.to_owned());
                set_tags(file, tags);
                put_on = true;
            }
        })?;
        if put_on {
            self.tags.put_on([tag.to_owned()])?;
        } else {
            self.tags.remember([tag.to_owned()])?;
        }
        Ok(bookmark)
    }

    /// Takes `tag` off the Bookmark `id`; one it doesn't have is as wanted.
    /// The Tag stays known. `NotFound` if there is no such Bookmark.
    pub fn remove_tag(&self, id: &str, tag: &str) -> io::Result<Bookmark> {
        self.modify(id, |file| {
            let mut tags = tags_of(file);
            tags.retain(|t| t != tag);
            set_tags(file, tags);
        })
    }

    /// Takes `tag` off every Bookmark, for the app's Delete a Tag, which also
    /// forgets the Tag itself. Repeating it after an interruption finishes the job.
    pub fn remove_tag_everywhere(&self, tag: &str) -> io::Result<()> {
        for bookmark in self.list_tagged(tag)? {
            self.remove_tag(&bookmark.id, tag)?;
        }
        Ok(())
    }

    /// Keeps `offset` (UTF-16, into `read_article`'s text; clamped to its end) as
    /// where the Bookmark `id` was left, and now as its Last Read. It replaces
    /// the previous one whichever way it points, so scrolling back and leaving
    /// moves Reading Progress back, as in Books (Books ADR-0004). `NotFound`
    /// if there is no such Bookmark or it has no Article.
    pub fn save_progress(&self, id: &str, offset: u64) -> io::Result<Bookmark> {
        let bookmark = self.read(id)?;
        if !bookmark.has_article {
            return Err(io::ErrorKind::NotFound.into());
        }
        let length = utf16_length(&self.read_article(id)?);
        let mut state = self.read_state(&bookmark.id);
        state.progress = Some((offset.min(length), now_ms() as u64));
        self.write_state(&bookmark.id, &state)?;
        self.read(id)
    }

    /// The Markup of the Bookmark `id`, oldest first: empty if it has none.
    /// A Highlight whose offsets no longer hold its quoted text is found by
    /// that text, and left out if it isn't there. `NotFound` if there is no
    /// such Bookmark or it has no Article.
    pub fn markup(&self, id: &str) -> io::Result<Markup> {
        let article = self.read_article(id)?;
        let mut markup = self.read_state(id).markup.listed();
        markup.highlights = resolve(&article, markup.highlights);
        let length = utf16_length(&article);
        for comment in &mut markup.page_comments {
            comment.place.offset = comment.place.offset.min(length);
        }
        Ok(markup)
    }

    /// Adds a Highlight of the passage `quoted_text` at `anchor` in `color`,
    /// `created` in milliseconds since the Unix epoch, to the Bookmark `id`, and
    /// returns it. `InvalidInput` for an `anchor` that is empty or past the
    /// Article's end; `NotFound` if there is no such Bookmark or it has no Article.
    pub fn add_highlight(
        &self,
        id: &str,
        anchor: Anchor,
        quoted_text: &str,
        color: Rgb,
        created: u64,
    ) -> io::Result<Highlight> {
        let highlight = Highlight {
            id: new_id(),
            anchor,
            quoted_text: quoted_text.to_owned(),
            color,
            created,
            comment: None,
        };
        self.edit_markup(id, |markup, _, length| {
            if anchor.start >= anchor.end || anchor.end > length {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            markup.highlights.push(highlight.clone());
            Ok(highlight)
        })
    }

    /// Sets the color of Highlight `highlight` of the Bookmark `id`; nothing if it isn't there.
    pub fn set_highlight_color(&self, id: &str, highlight: &str, color: Rgb) -> io::Result<()> {
        self.edit_markup(id, |m, changes, _| {
            m.set_highlight_color(changes, highlight, color, now_ms() as u64);
            Ok(())
        })
    }

    /// Sets the Comment of Highlight `highlight` of the Bookmark `id`; empty
    /// `comment` deletes it. Nothing if the Highlight isn't there.
    pub fn set_highlight_comment(
        &self,
        id: &str,
        highlight: &str,
        comment: &str,
    ) -> io::Result<()> {
        self.edit_markup(id, |m, changes, _| {
            m.set_highlight_comment(changes, highlight, comment, now_ms() as u64);
            Ok(())
        })
    }

    /// Removes Highlight `highlight`, with its Comment, from the Bookmark `id`.
    pub fn remove_highlight(&self, id: &str, highlight: &str) -> io::Result<()> {
        self.edit_markup(id, |m, changes, _| {
            m.remove_highlight(changes, highlight, now_ms() as u64);
            Ok(())
        })
    }

    /// Adds a Comment on the spot `place` of the Bookmark `id`, `created` in
    /// milliseconds since the Unix epoch, and returns it, still without text:
    /// it isn't in the Markup until `set_page_comment_text` gives it some. A
    /// `place` past the Article's end is its end.
    pub fn add_page_comment(
        &self,
        id: &str,
        place: Place,
        created: u64,
    ) -> io::Result<PageComment> {
        self.edit_markup(id, |markup, _, length| {
            let comment = PageComment {
                id: new_id(),
                place: Place {
                    offset: place.offset.min(length),
                },
                created,
                text: String::new(),
            };
            markup.page_comments.push(comment.clone());
            Ok(comment)
        })
    }

    /// Sets the text of page Comment `comment` of the Bookmark `id`; empty
    /// `text` deletes it. Nothing if it isn't there.
    pub fn set_page_comment_text(&self, id: &str, comment: &str, text: &str) -> io::Result<()> {
        self.edit_markup(id, |m, changes, _| {
            m.set_page_comment_text(changes, comment, text, now_ms() as u64);
            Ok(())
        })
    }

    /// Removes page Comment `comment` from the Bookmark `id`.
    pub fn remove_page_comment(&self, id: &str, comment: &str) -> io::Result<()> {
        self.edit_markup(id, |m, changes, _| {
            m.remove_page_comment(changes, comment, now_ms() as u64);
            Ok(())
        })
    }

    /// Applies `change` to the Markup of the Bookmark `id`, given the Article's
    /// length in UTF-16 units, and saves the state if that changed it.
    fn edit_markup<T>(
        &self,
        id: &str,
        change: impl FnOnce(&mut Markup, &mut MarkupChanges, u64) -> io::Result<T>,
    ) -> io::Result<T> {
        let length = utf16_length(&self.read_article(id)?);
        let mut state = self.read_state(id);
        let before = state.clone();
        let done = change(&mut state.markup, &mut state.changes, length)?;
        if state != before {
            self.write_state(id, &state)?;
        }
        Ok(done)
    }

    /// Gives the Bookmark `id` the `article` (see `save`, which also says what
    /// becomes of `images`), if it has none: an Article is never replaced, so
    /// that Markup stays where it was put. A blank `article` adds nothing.
    /// `NotFound` if there is no such Bookmark.
    pub fn add_article(
        &self,
        id: &str,
        article: &str,
        images: &HashMap<String, Vec<u8>>,
    ) -> io::Result<Bookmark> {
        let bookmark = self.read(id)?;
        if bookmark.has_article {
            return Ok(bookmark);
        }
        let body = self.keep_article(&mut Entropy::from_system(), article, images)?;
        self.modify(id, |file| file.body = body)
    }

    /// Takes the Article and its images off the Bookmark `id`, which keeps its
    /// details, Tags and Cover; one without an Article is as wanted.
    /// `NotFound` if there is no such Bookmark.
    pub fn drop_article(&self, id: &str) -> io::Result<Bookmark> {
        let images = self.images_of(&self.read(id)?);
        let bookmark = self.modify(id, |file| file.body.clear())?;
        ignore_not_found(self.files.remove(&self.state_path(&bookmark.id)))?;
        // After the file, so that an interruption leaves unused images, not a broken Article.
        for name in images {
            ignore_not_found(self.files.remove(&self.root.join(name)))?;
        }
        Ok(bookmark)
    }

    /// Applies `change` to the file of the Bookmark `id` and saves it, if that
    /// changed anything. `NotFound` if there is no such Bookmark.
    fn modify(&self, id: &str, change: impl FnOnce(&mut Document)) -> io::Result<Bookmark> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let mut file = self.read_file(&id).ok_or(io::ErrorKind::NotFound)?;
        let before = file.render();
        change(&mut file);
        if file.render() != before {
            file.set_scalar(UPDATED, iso8601(now_ms()));
            self.files.write(&self.path(&id), &file.render())?;
        }
        Ok(self.bookmark_of(&file, &id))
    }

    /// Settles what a Sync step left, given the ids of the Bookmarks it wrote
    /// or deleted here (`changed`; other names are skipped): Bookmarks that
    /// share an Address are merged (`merge_duplicates`), and of those in
    /// `changed` that Sync Deleted or Dropped the Article of, the state that
    /// another Device's Markup brought back is removed. Only those: a state
    /// may arrive a step before its Bookmark or its Article. Gives the ids of
    /// the Bookmarks it changed or removed.
    pub fn settle(&self, changed: &[String]) -> io::Result<Vec<String>> {
        let mut touched = self.merge_duplicates()?;
        for id in changed.iter().filter(|name| ItemId::parse(name).is_some()) {
            let state = self.state_path(id);
            if !self.files.exists(&state) || self.read(id).is_ok_and(|b| b.has_article) {
                continue;
            }
            ignore_not_found(self.files.remove(&state))?;
            touched.push(id.to_owned());
        }
        Ok(touched)
    }

    /// Merges the Bookmarks that share an Address, which two Devices Saving it
    /// before they Synced made, into the oldest Saved: Tags and Markup united,
    /// the Article kept from whichever has one (the oldest's if both do, the
    /// Highlights of the other found in it by their text). Gives the ids of
    /// the Bookmarks it changed or removed. Every Device picks the same one.
    fn merge_duplicates(&self) -> io::Result<Vec<String>> {
        let mut groups: BTreeMap<String, Vec<(ItemId, Document)>> = BTreeMap::new();
        for (id, file) in self.all_files()? {
            if let Some(address) = file.scalar(ADDRESS).and_then(normalise) {
                groups.entry(address).or_default().push((id, file));
            }
        }
        let mut touched = Vec::new();
        for mut group in groups.into_values().filter(|group| group.len() > 1) {
            group.sort_by(|(a, file_a), (b, file_b)| {
                let saved = |file: &Document| file.scalar(SAVED).unwrap_or_default().to_owned();
                (saved(file_a), a.as_str()).cmp(&(saved(file_b), b.as_str()))
            });
            let mut group = group.into_iter();
            let Some((id, mut oldest)) = group.next() else {
                continue;
            };
            for (other_id, other) in group {
                self.fold(&id, &mut oldest, &other_id, &other)?;
                touched.push(other_id.to_string());
            }
            touched.push(id.to_string());
        }
        Ok(touched)
    }

    /// Folds the Bookmark `other` into `kept` (see `merge_duplicates`) and
    /// removes it.
    fn fold(
        &self,
        id: &ItemId,
        kept: &mut Document,
        other_id: &ItemId,
        other: &Document,
    ) -> io::Result<()> {
        let mut state = self.read_state(id.as_str());
        let mut theirs = self.read_state(other_id.as_str());
        let before = state.clone();
        let mut spare = self.images_of(&unread_bookmark_of(other, other_id));
        let mut tags = tags_of(kept);
        for tag in tags_of(other) {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        set_tags(kept, tags.clone());
        if kept.body.is_empty() {
            // Their Article, its images and the Markup anchored in it.
            kept.body = other.body.clone();
            spare.clear();
        } else {
            let length = utf16_length(&kept.body);
            theirs.progress = None;
            theirs.markup.highlights = resolve(&kept.body, theirs.markup.highlights);
            theirs
                .markup
                .page_comments
                .retain(|c| c.place.offset <= length);
        }
        if kept.scalar(COVER).is_none()
            && let Some(cover) = other.scalar(COVER)
        {
            kept.set_scalar(COVER, cover);
        }
        state = state.merged(&theirs);
        kept.set_scalar(UPDATED, iso8601(now_ms()));
        self.files.write(&self.path(id), &kept.render())?;
        if state != before {
            self.write_state(id.as_str(), &state)?;
        }
        self.tags.remember(tags)?;
        let cover = other
            .scalar(COVER)
            .filter(|c| kept.scalar(COVER) != Some(c));
        for name in cover.map(str::to_owned).into_iter().chain(spare) {
            ignore_not_found(self.files.remove(&self.root.join(name)))?;
        }
        ignore_not_found(self.files.remove(&self.state_path(other_id.as_str())))?;
        ignore_not_found(self.files.remove(&self.path(other_id)))
    }

    /// Moves every Bookmark of `other` here, with its Tags, Cover and images, keeping when it was
    /// Saved: how the Share extension hands over what it Saved. A Bookmark whose Address is Saved
    /// here already only gives its Article, if this one has none (the other's Markup is none).
    /// Gives how many were taken.
    pub fn take_from(&self, other: &Bookmarks) -> io::Result<usize> {
        let mut mine = self.list()?;
        let mut entropy = Entropy::from_system();
        let mut taken = 0;
        for (id, mut file) in other.all_files()? {
            let theirs = unread_bookmark_of(&file, &id);
            let images = other.read_images(&theirs);
            match mine.iter().find(|b| b.address == theirs.address) {
                Some(known) if !known.has_article && theirs.has_article => {
                    self.add_article(&known.id, &file.body, &images)?;
                    taken += 1;
                }
                Some(_) => {}
                None => {
                    let new_id = self.unused_id(&mut entropy);
                    let cover = match theirs.cover.as_ref().and_then(|c| images.get(c)) {
                        Some(bytes) => self.keep_image(&mut entropy, bytes)?,
                        None => None,
                    };
                    file.set_scalar("id", new_id.as_str());
                    match cover {
                        Some(name) => file.set_scalar(COVER, name),
                        None => file.remove(COVER),
                    }
                    file.body = self.keep_article(&mut entropy, &file.body, &images)?;
                    self.files.write(&self.path(&new_id), &file.render())?;
                    self.tags.put_on(tags_of(&file))?;
                    mine.push(self.bookmark_of(&file, &new_id));
                    taken += 1;
                }
            }
            other.delete(&theirs.id)?;
        }
        Ok(taken)
    }

    /// The bytes of the Cover and of the images of the Article, by file name; those unreadable are left out.
    fn read_images(&self, bookmark: &Bookmark) -> HashMap<String, Vec<u8>> {
        (bookmark.cover.iter().cloned())
            .chain(self.images_of(bookmark))
            .filter_map(|name| {
                Some((
                    name.clone(),
                    self.files.read_bytes(&self.root.join(&name)).ok()?,
                ))
            })
            .collect()
    }

    /// Deletes the Bookmark `id` with its Cover and images; one that is gone
    /// already is as wanted.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        let Ok(bookmark) = self.read(id) else {
            return Ok(());
        };
        let images = self.images_of(&bookmark);
        for name in bookmark.cover.iter().chain(&images) {
            ignore_not_found(self.files.remove(&self.root.join(name)))?;
        }
        ignore_not_found(self.files.remove(&self.state_path(&bookmark.id)))?;
        ignore_not_found(
            self.files
                .remove(&self.root.join(format!("{}.md", bookmark.id))),
        )
    }

    /// The body for `article`: its images kept beside the Bookmarks and
    /// pointed at, those missing from `images` or no picture dropped; empty if
    /// the Article is blank.
    fn keep_article(
        &self,
        entropy: &mut Entropy,
        article: &str,
        images: &HashMap<String, Vec<u8>>,
    ) -> io::Result<String> {
        let mut kept: HashMap<String, Option<String>> = HashMap::new();
        let mut failed = None;
        let article = images::rewrite(article, |address| {
            kept.entry(address.to_owned())
                .or_insert_with(|| {
                    let bytes = images.get(address)?;
                    self.keep_image(entropy, bytes).unwrap_or_else(|e| {
                        failed = Some(e);
                        None
                    })
                })
                .clone()
        });
        if let Some(error) = failed {
            return Err(error);
        }
        let article = article.trim();
        Ok(if article.is_empty() {
            String::new()
        } else {
            format!("{article}\n")
        })
    }

    /// Writes `bytes` beside the Bookmarks as a new `<random-id>.<ext>`; none
    /// if they are no PNG, JPEG, GIF or WebP.
    fn keep_image(&self, entropy: &mut Entropy, bytes: &[u8]) -> io::Result<Option<String>> {
        let Some(extension) = extension(bytes) else {
            return Ok(None);
        };
        let name = format!("{}.{extension}", ItemId::new(entropy));
        self.files.write_bytes(&self.root.join(&name), bytes)?;
        Ok(Some(name))
    }

    /// The files of the images in the Bookmark's Article.
    fn images_of(&self, bookmark: &Bookmark) -> Vec<String> {
        let mut names = Vec::new();
        if let Ok(article) = self.read_article(&bookmark.id) {
            images::rewrite(&article, |name| {
                if is_image_file(name) {
                    names.push(name.to_owned());
                }
                Some(name.to_owned())
            });
        }
        names
    }

    fn path(&self, id: &ItemId) -> PathBuf {
        self.root.join(format!("{id}.md"))
    }

    /// A new Item ID that no Bookmark here has.
    fn unused_id(&self, entropy: &mut Entropy) -> ItemId {
        loop {
            let id = ItemId::new(entropy);
            if !self.files.exists(&self.path(&id)) {
                return id;
            }
        }
    }

    /// The hidden state file of `id`.
    fn state_path(&self, id: &str) -> PathBuf {
        self.root.join(STATE_FOLDER).join(format!("{id}.json"))
    }

    /// The state of `id`; an unreadable one reads as none.
    fn read_state(&self, id: &str) -> State {
        let text = self.files.read(&self.state_path(id)).unwrap_or_default();
        State::decode(&text)
    }

    fn write_state(&self, id: &str, state: &State) -> io::Result<()> {
        self.files.write(&self.state_path(id), &state.encode())
    }

    /// The Bookmark of `file`, with the Reading Progress its state file keeps.
    fn bookmark_of(&self, file: &Document, id: &ItemId) -> Bookmark {
        let mut bookmark = unread_bookmark_of(file, id);
        if bookmark.has_article {
            bookmark.progress = self
                .read_state(id.as_str())
                .progress
                .map(|(offset, last_read)| progress_of(offset, last_read, &file.body));
        }
        bookmark
    }

    /// Every file that is a Bookmark, in no order.
    fn all_files(&self) -> io::Result<Vec<(ItemId, Document)>> {
        let mut all = Vec::new();
        for listed in self.files.list(&self.root)? {
            let Some(id) = (listed.name.strip_suffix(".md")).and_then(ItemId::parse) else {
                continue;
            };
            if let Some(file) = self.read_file(&id) {
                all.push((id, file));
            }
        }
        Ok(all)
    }

    /// The file of `id`, none if it is none, which includes a file whose
    /// front matter names another `id` or no Address.
    fn read_file(&self, id: &ItemId) -> Option<Document> {
        let text = self.files.read(&self.path(id)).ok()?;
        Document::parse(&text)
            .filter(|file| file.scalar("id") == Some(id.as_str()) && file.scalar(ADDRESS).is_some())
    }
}

/// `text` with its whitespace, line breaks included, as single spaces.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The valid Tags of a file, in order.
fn tags_of(file: &Document) -> Vec<String> {
    let tags = file.list(TAGS).unwrap_or_default();
    tags.iter().filter(|t| tag::is_valid(t)).cloned().collect()
}

/// An empty list is no property at all.
fn set_tags(file: &mut Document, tags: Vec<String>) {
    if tags.is_empty() {
        file.remove(TAGS);
    } else {
        file.set(TAGS, Value::List(tags));
    }
}

fn ignore_not_found(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

fn unread_bookmark_of(file: &Document, id: &ItemId) -> Bookmark {
    let text = |key| file.scalar(key).unwrap_or_default().to_owned();
    Bookmark {
        id: id.as_str().to_owned(),
        address: text(ADDRESS),
        title: text(TITLE),
        site: text(SITE),
        cover: file
            .scalar(COVER)
            .filter(|c| is_image_file(c))
            .map(str::to_owned),
        excerpt: text(EXCERPT),
        author: text(AUTHOR),
        published: text(PUBLISHED),
        saved: text(SAVED),
        has_article: !file.body.trim().is_empty(),
        tags: tags_of(file),
        progress: None,
    }
}

/// UTF-16 code units in `text`: the unit of offsets into an Article (Books ADR-0006).
fn utf16_length(text: &str) -> u64 {
    text.encode_utf16().count() as u64
}

/// The Reading Progress at `offset` in an Article `body`, never past its end.
fn progress_of(offset: u64, last_read: u64, body: &str) -> ArticleProgress {
    let length = utf16_length(body);
    let offset = offset.min(length);
    ArticleProgress {
        offset,
        last_read,
        percent: (offset * 100).checked_div(length).unwrap_or(0) as u8,
    }
}

/// `highlights` where their quoted text is in `article`: each as it is if its
/// offsets hold the text, else at the first place the text is; those whose
/// text isn't there are left out.
fn resolve(article: &str, highlights: Vec<Highlight>) -> Vec<Highlight> {
    let units: Vec<u16> = article.encode_utf16().collect();
    let holds = |h: &Highlight| {
        (units.get(h.anchor.start as usize..h.anchor.end as usize))
            .is_some_and(|held| held.iter().copied().eq(h.quoted_text.encode_utf16()))
    };
    let mut resolved = Vec::with_capacity(highlights.len());
    for mut highlight in highlights {
        if !holds(&highlight) {
            let quoted = &highlight.quoted_text;
            let Some(found) = article.find(quoted.as_str()).filter(|_| !quoted.is_empty()) else {
                continue;
            };
            let start = utf16_length(&article[..found]);
            highlight.anchor = Anchor {
                start,
                end: start + utf16_length(quoted),
            };
        }
        resolved.push(highlight);
    }
    resolved
}

/// A new Item ID, for a Highlight or a page Comment.
fn new_id() -> String {
    ItemId::new(&mut Entropy::from_system()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::article::read_article;
    use pkm_base::tags::Tags;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest";

    /// A Bookmarks folder in a temporary directory, removed afterwards.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let id = ItemId::new(&mut Entropy::from_system());
            Self(
                std::env::temp_dir()
                    .join(format!("pkm-bookmarks-{id}"))
                    .join("Bookmarks"),
            )
        }

        fn store(&self) -> Bookmarks {
            Bookmarks::open(&self.0).unwrap()
        }

        fn files(&self) -> Vec<String> {
            let mut names: Vec<_> = fs::read_dir(&self.0)
                .unwrap()
                .map(|e| e.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(self.0.parent().unwrap()));
        }
    }

    /// Where the Cover of `details` is.
    const COVER: &str = "https://i.test/cover.png";

    fn details(title: &str) -> Details {
        Details {
            title: title.into(),
            cover: Some(COVER.into()),
            site: "Site".into(),
            excerpt: "About".into(),
            author: "Ann".into(),
            published: "2024-05-01".into(),
        }
    }

    #[test]
    fn taking_from_another_folder_moves_its_bookmarks_with_tags_and_images() {
        let (here, there) = (Scratch::new(), Scratch::new());
        let theirs = there.store();
        let article = "A\n\n![fig](https://i.test/fig.png)\n";
        let saved = theirs
            .save(
                "https://x.test/new",
                &details("New"),
                "",
                &["rust".to_owned()],
                Some(article),
                &downloaded(&[(COVER, PNG), ("https://i.test/fig.png", PNG)]),
            )
            .unwrap()
            .bookmark;
        let mine = here.store();
        assert_eq!(mine.take_from(&theirs).unwrap(), 1);
        assert!(theirs.list().unwrap().is_empty());
        let taken = mine.list().unwrap().remove(0);
        assert_eq!(
            (taken.address.as_str(), taken.title.as_str()),
            ("https://x.test/new", "New")
        );
        assert_eq!(
            (taken.tags.clone(), taken.saved),
            (vec!["rust".into()], saved.saved)
        );
        let name = taken.cover.unwrap();
        assert_eq!(mine.read_cover(&taken.id).unwrap(), PNG);
        assert!(here.files().contains(&name));
        let text = mine.read_article(&taken.id).unwrap();
        let image = text.split(['(', ')']).nth(1).unwrap();
        assert_eq!(mine.read_image(&taken.id, image).unwrap(), PNG);
        assert!(there.files().iter().all(|f| !is_image_file(f)));
    }

    #[test]
    fn taking_an_address_saved_here_adds_only_a_missing_article() {
        let (here, there) = (Scratch::new(), Scratch::new());
        let (mine, theirs) = (here.store(), there.store());
        let plain = |s: &Bookmarks, article: Option<&str>| {
            s.save(
                "https://x.test/a",
                &details("A"),
                "",
                &[],
                article,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark
        };
        let kept = plain(&mine, None);
        plain(&theirs, Some("Text\n"));
        assert_eq!(mine.take_from(&theirs).unwrap(), 1);
        assert_eq!(mine.list().unwrap().len(), 1);
        assert_eq!(mine.read_article(&kept.id).unwrap(), "Text\n");
        plain(&theirs, Some("Other\n"));
        mine.take_from(&theirs).unwrap();
        assert_eq!(mine.read_article(&kept.id).unwrap(), "Text\n");
        assert!(theirs.list().unwrap().is_empty());
    }

    #[test]
    fn a_saved_page_is_a_markdown_file_named_by_its_id_with_its_details() {
        let scratch = Scratch::new();
        let saved = scratch
            .store()
            .save(
                "https://Example.com/a?utm_source=x#top",
                &details("Page"),
                "",
                &[],
                None,
                &HashMap::new(),
            )
            .unwrap();
        assert!(!saved.existing);
        let b = saved.bookmark;
        assert_eq!(b.address, "https://example.com/a");
        assert_eq!(
            (b.title.as_str(), b.site.as_str(), b.author.as_str()),
            ("Page", "Site", "Ann")
        );
        let text = fs::read_to_string(scratch.0.join(format!("{}.md", b.id))).unwrap();
        assert!(text.starts_with(&format!(
            "---\nid: {}\naddress: https://example.com/a\ntitle: Page\nsite: Site\nexcerpt: About\nauthor: Ann\npublished: 2024-05-01\nsaved: 20",
            b.id
        )));
        assert!(text.ends_with("Z\n---\n"));
        assert_eq!(scratch.store().read(&b.id).unwrap(), b);
    }

    #[test]
    fn the_users_title_wins_and_a_blank_one_falls_back_to_the_page_then_the_address() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let title = |address: &str, details: &Details, typed: &str| {
            store
                .save(address, details, typed, &[], None, &HashMap::new())
                .unwrap()
                .bookmark
                .title
        };
        assert_eq!(
            title("https://a.test/1", &details("Page"), "Mine\n  now"),
            "Mine now"
        );
        assert_eq!(title("https://a.test/2", &details("Page"), "  "), "Page");
        assert_eq!(
            title("https://a.test/3", &Details::default(), ""),
            "https://a.test/3"
        );
    }

    #[test]
    fn saving_an_address_saved_already_gives_that_bookmark() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let first = store
            .save(
                "https://example.com/a",
                &details("One"),
                "",
                &[],
                None,
                &downloaded(&[(COVER, PNG)]),
            )
            .unwrap();
        let again = store
            .save(
                "HTTPS://example.com/a?utm_medium=mail#x",
                &details("Two"),
                "",
                &[],
                None,
                &HashMap::new(),
            )
            .unwrap();
        assert!(again.existing);
        assert_eq!(again.bookmark, first.bookmark);
        assert_eq!(store.list().unwrap().len(), 1);
    }

    #[test]
    fn an_address_that_is_none_is_refused() {
        let scratch = Scratch::new();
        let error = scratch
            .store()
            .save(
                "ftp://x.test/a",
                &details("X"),
                "",
                &[],
                None,
                &HashMap::new(),
            )
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(scratch.files().is_empty());
    }

    #[test]
    fn the_list_is_newest_saved_first_and_skips_other_files() {
        let scratch = Scratch::new();
        let store = scratch.store();
        for (id, saved) in [
            ("aaaaaaaaaaaa", "2024-01-01T00:00:00Z"),
            ("bbbbbbbbbbbb", "2024-03-01T00:00:00Z"),
            ("cccccccccccc", "2024-02-01T00:00:00Z"),
        ] {
            let text = format!(
                "---\nid: {id}\naddress: https://x.test/{id}\ntitle: {id}\nsaved: {saved}\n---\n"
            );
            fs::write(scratch.0.join(format!("{id}.md")), text).unwrap();
        }
        fs::write(scratch.0.join("notes.md"), "x").unwrap();
        fs::write(
            scratch.0.join("dddddddddddd.md"),
            "---\nid: eeeeeeeeeeee\n---\n",
        )
        .unwrap();
        let ids: Vec<_> = store.list().unwrap().into_iter().map(|b| b.id).collect();
        assert_eq!(ids, ["bbbbbbbbbbbb", "cccccccccccc", "aaaaaaaaaaaa"]);
    }

    #[test]
    fn the_cover_is_kept_beside_the_bookmark_and_read_back() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save(
                "https://a.test/",
                &details("A"),
                "",
                &[],
                None,
                &downloaded(&[(COVER, PNG)]),
            )
            .unwrap()
            .bookmark;
        let cover = b.cover.clone().unwrap();
        assert!(is_image_file(&cover) && cover.ends_with(".png"));
        assert!(scratch.files().contains(&cover));
        assert_eq!(store.read_cover(&b.id).unwrap(), PNG);
    }

    #[test]
    fn a_cover_that_is_no_image_is_left_out_without_failing_the_save() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save(
                "https://a.test/",
                &details("A"),
                "",
                &[],
                None,
                &downloaded(&[(COVER, b"<html>")]),
            )
            .unwrap()
            .bookmark;
        assert_eq!(b.cover, None);
        assert_eq!(
            store.read_cover(&b.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(scratch.files().len(), 1);
    }

    #[test]
    fn deleting_takes_the_cover_and_deleting_again_is_fine() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let keep = store
            .save(
                "https://a.test/keep",
                &details("Keep"),
                "",
                &[],
                None,
                &downloaded(&[(COVER, PNG)]),
            )
            .unwrap()
            .bookmark;
        let gone = store
            .save(
                "https://a.test/gone",
                &details("Gone"),
                "",
                &[],
                None,
                &downloaded(&[(COVER, PNG)]),
            )
            .unwrap()
            .bookmark;
        store.delete(&gone.id).unwrap();
        store.delete(&gone.id).unwrap();
        assert_eq!(
            store.read(&gone.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(scratch.files().len(), 2);
        assert_eq!(store.list().unwrap(), [keep]);
    }

    #[test]
    fn an_article_is_the_body_of_the_file_and_marks_its_bookmark() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let with = store
            .save(
                "https://a.test/1",
                &details("One"),
                "",
                &[],
                Some("# Head\n\nText\n\n"),
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        let without = store
            .save(
                "https://a.test/2",
                &details("Two"),
                "",
                &[],
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        assert!(with.has_article && !without.has_article);
        assert_eq!(store.read_article(&with.id).unwrap(), "# Head\n\nText\n");
        assert_eq!(
            store.read_article(&without.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        let text = fs::read_to_string(scratch.0.join(format!("{}.md", with.id))).unwrap();
        assert!(text.ends_with("Z\n---\n# Head\n\nText\n"));
        let listed = store.list().unwrap();
        assert_eq!(listed.iter().filter(|b| b.has_article).count(), 1);
    }

    #[test]
    fn the_user_can_override_what_the_page_reads_like() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let long = format!(
            "<article><p>{}</p></article>",
            "Plenty of prose, indeed. ".repeat(30)
        );
        let short = "<p>Home, sweet home, welcome, friends.</p>";
        let save = |address: &str, html: &str, keep: Option<bool>| {
            let article = read_article(html, address);
            let keep = keep.unwrap_or(article.reads_like_article);
            store
                .save(
                    address,
                    &details("T"),
                    "",
                    &[],
                    keep.then_some(article.markdown.as_str()),
                    &HashMap::new(),
                )
                .unwrap()
                .bookmark
                .has_article
        };
        assert!(save("https://a.test/long", &long, None));
        assert!(!save("https://a.test/long-off", &long, Some(false)));
        assert!(!save("https://a.test/short", short, None));
        assert!(save("https://a.test/short-on", short, Some(true)));
    }

    #[test]
    fn a_blank_article_is_none() {
        let scratch = Scratch::new();
        let saved = scratch
            .store()
            .save(
                "https://a.test/1",
                &details("One"),
                "",
                &[],
                Some(" \n"),
                &HashMap::new(),
            )
            .unwrap();
        assert!(!saved.bookmark.has_article);
    }

    #[test]
    fn deleting_takes_the_article_with_the_file() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save(
                "https://a.test/1",
                &details("One"),
                "",
                &[],
                Some("Text"),
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        store.delete(&b.id).unwrap();
        assert!(scratch.files().is_empty());
    }

    const JPEG: &[u8] = b"\xff\xd8\xff\xe0 rest";

    fn downloaded(pairs: &[(&str, &[u8])]) -> HashMap<String, Vec<u8>> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), b.to_vec()))
            .collect()
    }

    fn saved_with_images(store: &Bookmarks, article: &str, images: &[(&str, &[u8])]) -> Bookmark {
        store
            .save(
                "https://x.test/a",
                &details("A"),
                "",
                &[],
                Some(article),
                &downloaded(images),
            )
            .unwrap()
            .bookmark
    }

    #[test]
    fn article_images_are_kept_beside_the_bookmark_and_referenced() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let article = "Intro\n\n![one](https://i.test/1.png)\n\n![two](https://i.test/2.jpg)\n\n![again](https://i.test/1.png)\n";
        let b = saved_with_images(
            &store,
            article,
            &[
                ("https://i.test/1.png", PNG),
                ("https://i.test/2.jpg", JPEG),
            ],
        );
        let text = store.read_article(&b.id).unwrap();
        let names = |ext: &str| -> Vec<String> {
            text.split("](")
                .skip(1)
                .map(|r| r.split(')').next().unwrap().to_owned())
                .filter(|n| n.ends_with(ext))
                .collect()
        };
        let (png, jpg) = (names(".png"), names(".jpg"));
        assert_eq!((png.len(), jpg.len()), (2, 1));
        assert_eq!(png[0], png[1], "one download, one file");
        assert!(!text.contains("https://i.test"));
        assert!(is_image_file(&png[0]) && is_image_file(&jpg[0]));
        assert_eq!(store.read_image(&b.id, &png[0]).unwrap(), PNG);
        assert_eq!(store.read_image(&b.id, &jpg[0]).unwrap(), JPEG);
        assert_eq!(scratch.files().len(), 3);
    }

    #[test]
    fn an_image_that_was_not_downloaded_or_is_none_is_dropped_without_failing_the_save() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let article =
            "Intro\n\n![gone](https://i.test/gone.png)\n\n![bad](https://i.test/bad.png)\n\nEnd\n";
        let b = saved_with_images(&store, article, &[("https://i.test/bad.png", b"<html>")]);
        assert_eq!(store.read_article(&b.id).unwrap(), "Intro\n\nEnd\n");
        assert_eq!(scratch.files().len(), 1);
    }

    #[test]
    fn an_image_can_only_be_read_if_the_article_uses_it() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let a = saved_with_images(
            &store,
            "![](https://i.test/1.png)\n",
            &[("https://i.test/1.png", PNG)],
        );
        let b = store
            .save(
                "https://x.test/b",
                &details("B"),
                "",
                &[],
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        let name = store.read_article(&a.id).unwrap();
        let name = name.trim().trim_start_matches("![](").trim_end_matches(')');
        assert!(store.read_image(&a.id, name).is_ok());
        assert!(store.read_image(&b.id, name).is_err());
        assert!(store.read_image(&a.id, "../x.png").is_err());
    }

    #[test]
    fn deleting_takes_the_article_images_and_the_cover_with_it() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save(
                "https://x.test/a",
                &details("A"),
                "",
                &[],
                Some("![](https://i.test/1.png)\n"),
                &downloaded(&[(COVER, PNG), ("https://i.test/1.png", JPEG)]),
            )
            .unwrap()
            .bookmark;
        let other = store
            .save(
                "https://x.test/b",
                &details("B"),
                "",
                &[],
                Some("![](https://i.test/2.png)\n"),
                &downloaded(&[("https://i.test/2.png", PNG)]),
            )
            .unwrap()
            .bookmark;
        assert_eq!(scratch.files().len(), 5);
        store.delete(&b.id).unwrap();
        assert_eq!(scratch.files().len(), 2);
        assert!(scratch.files().iter().any(|f| f.starts_with(&other.id)));
    }

    /// A Bookmark of `address`, without Article.
    fn plain(store: &Bookmarks, address: &str) -> Bookmark {
        store
            .save(address, &details("T"), "", &[], None, &HashMap::new())
            .unwrap()
            .bookmark
    }

    fn tags_file(scratch: &Scratch) -> String {
        fs::read_to_string(scratch.0.parent().unwrap().join("Tags/tags.json")).unwrap_or_default()
    }

    #[test]
    fn a_bookmark_can_be_renamed_and_a_blank_name_changes_nothing() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = plain(&store, "https://a.test/1");
        assert_eq!(
            store.rename(&b.id, " Mine\n now ").unwrap().title,
            "Mine now"
        );
        assert_eq!(store.read(&b.id).unwrap().title, "Mine now");
        assert_eq!(store.rename(&b.id, "  ").unwrap().title, "Mine now");
        assert_eq!(
            store.rename("nope", "x").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn tags_are_added_once_in_order_removed_and_join_the_tags_file() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = plain(&store, "https://a.test/1");
        store.add_tag(&b.id, "work").unwrap();
        store.add_tag(&b.id, "home").unwrap();
        assert_eq!(store.add_tag(&b.id, "work").unwrap().tags, ["work", "home"]);
        assert!(
            tags_file(&scratch).contains("\"work\"") && tags_file(&scratch).contains("\"home\"")
        );
        let text = fs::read_to_string(scratch.0.join(format!("{}.md", b.id))).unwrap();
        assert!(text.contains("tags:\n  - work\n  - home\n"));
        assert_eq!(store.remove_tag(&b.id, "work").unwrap().tags, ["home"]);
        assert_eq!(store.read(&b.id).unwrap().tags, ["home"]);
        // Removing it from the Bookmark leaves the Tag known.
        assert!(tags_file(&scratch).contains("\"work\""));
        assert_eq!(store.remove_tag(&b.id, "work").unwrap().tags, ["home"]);
        assert_eq!(
            store.add_tag(&b.id, "Not A Tag").unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn tags_put_on_a_bookmark_raise_their_relevance_and_order_those_in_use() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let tags = ["a", "b"].map(String::from);
        let one = store
            .save(
                "https://a.test/1",
                &details("T"),
                "",
                &tags,
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        let two = plain(&store, "https://a.test/2");
        store.add_tag(&two.id, "b").unwrap();
        store.add_tag(&two.id, "b").unwrap(); // carried already
        store.remove_tag(&one.id, "a").unwrap();
        store.add_tag(&two.id, "c").unwrap();
        let known = Tags::parse(&tags_file(&scratch));
        assert_eq!([known.relevance("a"), known.relevance("b")], [1, 2]);
        assert_eq!(store.tags_in_use().unwrap(), ["b", "c"]);
    }

    #[test]
    fn saving_puts_the_valid_tags_on_once_and_they_join_the_tags_file() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let tags = ["work", "Not A Tag", "home", "work"].map(String::from);
        let b = store
            .save(
                "https://a.test/1",
                &details("T"),
                "",
                &tags,
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        assert_eq!(b.tags, ["work", "home"]);
        assert_eq!(store.read(&b.id).unwrap().tags, ["work", "home"]);
        assert!(tags_file(&scratch).contains("\"home\""));
        let again = store
            .save(
                "https://a.test/1",
                &details("T"),
                "",
                &["other".into()],
                None,
                &HashMap::new(),
            )
            .unwrap();
        assert!(again.existing && again.bookmark.tags == ["work", "home"]);
    }

    #[test]
    fn the_list_can_be_narrowed_to_a_tag() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let a = plain(&store, "https://a.test/1");
        let b = plain(&store, "https://a.test/2");
        store.add_tag(&b.id, "work").unwrap();
        store.add_tag(&a.id, "work/x").unwrap();
        let ids = |tag| -> Vec<String> {
            store
                .list_tagged(tag)
                .unwrap()
                .into_iter()
                .map(|b| b.id)
                .collect()
        };
        assert_eq!(ids("work"), [b.id]);
        assert!(ids("none").is_empty());
    }

    #[test]
    fn deleting_a_tag_removes_it_from_every_bookmark_but_leaves_the_others() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let a = plain(&store, "https://a.test/1");
        let b = plain(&store, "https://a.test/2");
        for tag in ["x", "y"] {
            store.add_tag(&a.id, tag).unwrap();
        }
        store.add_tag(&b.id, "x").unwrap();
        store.remove_tag_everywhere("x").unwrap();
        assert_eq!(store.read(&a.id).unwrap().tags, ["y"]);
        assert!(store.read(&b.id).unwrap().tags.is_empty());
    }

    #[test]
    fn an_article_is_added_once_with_its_images_and_never_replaced() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = plain(&store, "https://a.test/1");
        let added = store
            .add_article(
                &b.id,
                "Text\n\n![i](https://i.test/1.png)\n\n![j](https://i.test/2.png)\n",
                &downloaded(&[("https://i.test/1.png", PNG)]),
            )
            .unwrap();
        assert!(added.has_article);
        let text = store.read_article(&b.id).unwrap();
        assert!(
            text.starts_with("Text\n")
                && !text.contains("![j]") // a random file name may end in "2.png"
                && !text.contains("https://i.test/1")
        );
        let again = store
            .add_article(&b.id, "Other\n", &HashMap::new())
            .unwrap();
        assert!(again.has_article);
        assert_eq!(store.read_article(&b.id).unwrap(), text);
        assert_eq!(store.read(&b.id).unwrap().title, "T");
    }

    #[test]
    fn a_blank_article_is_not_added() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = plain(&store, "https://a.test/1");
        assert!(
            !store
                .add_article(&b.id, " \n", &HashMap::new())
                .unwrap()
                .has_article
        );
    }

    #[test]
    fn dropping_the_article_keeps_the_bookmark_its_tags_and_cover() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = store
            .save(
                "https://x.test/a",
                &details("A"),
                "",
                &[],
                Some("Text\n\n![i](https://i.test/1.png)\n"),
                &downloaded(&[(COVER, PNG), ("https://i.test/1.png", PNG)]),
            )
            .unwrap()
            .bookmark;
        store.add_tag(&b.id, "keep").unwrap();
        let dropped = store.drop_article(&b.id).unwrap();
        assert!(!dropped.has_article);
        assert_eq!(
            (dropped.title.as_str(), &dropped.tags[..]),
            ("A", &["keep".to_owned()][..])
        );
        assert_eq!(scratch.files().len(), 2, "{:?}", scratch.files());
        assert!(store.read_cover(&b.id).is_ok());
        assert_eq!(
            store.read_article(&b.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(!store.drop_article(&b.id).unwrap().has_article);
    }

    /// A Bookmark whose Article is 10 UTF-16 units long: nine and a line break.
    fn tenth_part_article(store: &Bookmarks) -> Bookmark {
        saved_with_images(store, "123456789", &[])
    }

    #[test]
    fn reading_progress_and_last_read_round_trip_as_a_percentage_of_the_article() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = tenth_part_article(&store);
        assert_eq!(b.progress, None, "never read");
        let before = now_ms() as u64;
        let saved = store.save_progress(&b.id, 5).unwrap().progress.unwrap();
        assert_eq!((saved.offset, saved.percent), (5, 50));
        assert!(saved.last_read >= before);
        let reopened = scratch.store();
        assert_eq!(reopened.read(&b.id).unwrap().progress, Some(saved));
        assert_eq!(reopened.list().unwrap()[0].progress, Some(saved));
    }

    #[test]
    fn progress_counts_utf16_units_and_moves_back_when_saved_earlier() {
        let scratch = Scratch::new();
        let store = scratch.store();
        // The emoji is two UTF-16 units: 4 + 2 + 3 + the line break.
        let b = saved_with_images(&store, "abcd😀efg", &[]);
        assert_eq!(
            store
                .save_progress(&b.id, 9)
                .unwrap()
                .progress
                .unwrap()
                .percent,
            90
        );
        let back = store.save_progress(&b.id, 3).unwrap().progress.unwrap();
        assert_eq!((back.offset, back.percent), (3, 30));
    }

    #[test]
    fn progress_past_the_end_stops_at_the_end() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = tenth_part_article(&store);
        let p = store.save_progress(&b.id, 999).unwrap().progress.unwrap();
        assert_eq!((p.offset, p.percent), (10, 100));
    }

    #[test]
    fn a_bookmark_never_read_has_no_state_file_and_one_without_article_cannot_be_read() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = tenth_part_article(&store);
        assert!(!scratch.0.join(".state").exists());
        let plain = plain(&store, "https://x.test/plain");
        assert_eq!(
            store.save_progress(&plain.id, 1).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            store.save_progress("nonsense", 1).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(store.read(&b.id).unwrap().progress, None);
    }

    #[test]
    fn an_unreadable_state_reads_as_never_read_and_the_next_save_replaces_it() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = tenth_part_article(&store);
        fs::create_dir_all(scratch.0.join(".state")).unwrap();
        fs::write(
            scratch.0.join(".state").join(format!("{}.json", b.id)),
            "junk",
        )
        .unwrap();
        assert_eq!(store.read(&b.id).unwrap().progress, None);
        assert_eq!(
            store
                .save_progress(&b.id, 2)
                .unwrap()
                .progress
                .unwrap()
                .offset,
            2
        );
    }

    #[test]
    fn dropping_the_article_and_deleting_take_the_state_file() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let state = |id: &str| scratch.0.join(".state").join(format!("{id}.json"));
        let a = tenth_part_article(&store);
        store.save_progress(&a.id, 4).unwrap();
        assert!(state(&a.id).exists());
        assert_eq!(store.drop_article(&a.id).unwrap().progress, None);
        assert!(!state(&a.id).exists());
        let b = store
            .save(
                "https://x.test/b",
                &details("B"),
                "",
                &[],
                Some("text"),
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        store.save_progress(&b.id, 1).unwrap();
        store.delete(&b.id).unwrap();
        assert!(!state(&b.id).exists());
    }

    const YELLOW: Rgb = Rgb {
        r: 255,
        g: 235,
        b: 60,
    };
    const GREEN: Rgb = Rgb {
        r: 126,
        g: 217,
        b: 87,
    };

    /// A Bookmark whose Article is "alpha beta\ngamma delta\n" (23 UTF-16 units).
    fn two_paragraphs(store: &Bookmarks) -> Bookmark {
        saved_with_images(store, "alpha beta\n\ngamma delta", &[])
    }

    fn highlight_at(store: &Bookmarks, id: &str, start: u64, end: u64, quoted: &str) -> Highlight {
        let anchor = Anchor { start, end };
        store.add_highlight(id, anchor, quoted, YELLOW, 1).unwrap()
    }

    #[test]
    fn highlights_can_span_paragraphs_and_be_recolored_and_deleted() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = two_paragraphs(&store);
        let article = store.read_article(&b.id).unwrap();
        let passage = &article[6..18];
        let h = highlight_at(&store, &b.id, 6, 18, passage);
        assert!(passage.contains("\n\n"));
        store.set_highlight_color(&b.id, &h.id, GREEN).unwrap();
        let listed = scratch.store().markup(&b.id).unwrap();
        assert_eq!(listed.highlights.len(), 1);
        assert_eq!(
            (listed.highlights[0].anchor, listed.highlights[0].color),
            (Anchor { start: 6, end: 18 }, GREEN)
        );
        store.remove_highlight(&b.id, &h.id).unwrap();
        assert!(scratch.store().markup(&b.id).unwrap().highlights.is_empty());
    }

    #[test]
    fn comments_on_highlights_and_spots_are_written_and_deleted() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = two_paragraphs(&store);
        let h = highlight_at(&store, &b.id, 0, 5, "alpha");
        store.set_highlight_comment(&b.id, &h.id, "why").unwrap();
        let spot = store
            .add_page_comment(&b.id, Place { offset: 12 }, 2)
            .unwrap();
        assert!(
            store.markup(&b.id).unwrap().page_comments.is_empty(),
            "without text it isn't listed"
        );
        store.set_page_comment_text(&b.id, &spot.id, "hm").unwrap();
        let markup = scratch.store().markup(&b.id).unwrap();
        assert_eq!(markup.highlights[0].comment.as_deref(), Some("why"));
        assert_eq!(
            (
                markup.page_comments[0].place,
                &markup.page_comments[0].text[..]
            ),
            (Place { offset: 12 }, "hm")
        );
        store.set_highlight_comment(&b.id, &h.id, "").unwrap();
        store.set_page_comment_text(&b.id, &spot.id, "").unwrap();
        let markup = scratch.store().markup(&b.id).unwrap();
        assert_eq!(markup.highlights[0].comment, None);
        assert!(markup.page_comments.is_empty());
    }

    #[test]
    fn markup_keeps_reading_progress_and_the_other_way_round() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = two_paragraphs(&store);
        highlight_at(&store, &b.id, 0, 5, "alpha");
        store.save_progress(&b.id, 7).unwrap();
        highlight_at(&store, &b.id, 6, 10, "beta");
        assert_eq!(store.read(&b.id).unwrap().progress.unwrap().offset, 7);
        assert_eq!(store.markup(&b.id).unwrap().highlights.len(), 2);
    }

    #[test]
    fn a_highlight_that_moved_is_found_by_its_text_and_one_that_is_gone_is_left_out() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = two_paragraphs(&store);
        highlight_at(&store, &b.id, 0, 4, "beta");
        highlight_at(&store, &b.id, 0, 4, "omega");
        let listed = store.markup(&b.id).unwrap();
        assert_eq!(listed.highlights.len(), 1);
        assert_eq!(listed.highlights[0].anchor, Anchor { start: 6, end: 10 });
    }

    #[test]
    fn markup_must_lie_inside_the_article_of_a_bookmark_that_has_one() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = two_paragraphs(&store);
        let anchor = |start, end| Anchor { start, end };
        for bad in [anchor(3, 3), anchor(5, 2), anchor(0, 999)] {
            let err = store.add_highlight(&b.id, bad, "x", YELLOW, 1).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        }
        let plain = store
            .save(
                "https://x.test/p",
                &details("P"),
                "",
                &[],
                None,
                &HashMap::new(),
            )
            .unwrap()
            .bookmark;
        let err = store.add_highlight(&plain.id, anchor(0, 1), "x", YELLOW, 1);
        assert_eq!(err.unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(
            store.markup(&plain.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn dropping_the_article_and_deleting_clear_the_markup() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = two_paragraphs(&store);
        highlight_at(&store, &b.id, 0, 5, "alpha");
        store.drop_article(&b.id).unwrap();
        assert!(
            !scratch
                .0
                .join(".state")
                .join(format!("{}.json", b.id))
                .exists()
        );
        store
            .add_article(&b.id, "alpha again", &HashMap::new())
            .unwrap();
        assert!(store.markup(&b.id).unwrap().highlights.is_empty());
        highlight_at(&store, &b.id, 0, 5, "alpha");
        store.delete(&b.id).unwrap();
        assert!(
            !scratch
                .0
                .join(".state")
                .join(format!("{}.json", b.id))
                .exists()
        );
    }

    fn saved_with_article(
        store: &Bookmarks,
        address: &str,
        title: &str,
        article: &str,
    ) -> Bookmark {
        let mut page = details(title);
        page.cover = None;
        let saved = store.save(address, &page, "", &[], Some(article), &HashMap::new());
        saved.unwrap().bookmark
    }

    #[test]
    fn search_ignores_case_and_accents_and_finds_substrings_in_every_field() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let by_title = saved_with_article(&store, "https://a.test/x", "Un Café", "");
        let by_article = saved_with_article(&store, "https://b.test/x", "B", "some **paragraph**");
        let by_address = saved_with_article(&store, "https://rust-lang.test/x", "C", "");
        let found = |query: &str| -> Vec<String> {
            let found = store.search(query).unwrap();
            found.into_iter().map(|f| f.bookmark.id).collect()
        };
        assert_eq!(found("CAFE"), vec![by_title.id.clone()]);
        assert_eq!(found("graph"), vec![by_article.id.clone()]);
        assert_eq!(found("**para"), vec![by_article.id]);
        assert_eq!(found("rust-LANG"), vec![by_address.id]);
        assert_eq!(found("site").len(), 3, "the Site of all");
        assert!(found("  ").is_empty());
        assert!(found("absent").is_empty());
    }

    #[test]
    fn search_lists_title_matches_first_then_newest_saved() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let mut ids = Vec::new();
        for (n, (title, article)) in [("x", "tea"), ("Tea", ""), ("x", "tea"), ("y", "")]
            .into_iter()
            .enumerate()
        {
            let address = format!("https://t.test/{n}");
            let id = saved_with_article(&store, &address, title, article).id;
            let path = scratch.0.join(format!("{id}.md"));
            let text = fs::read_to_string(&path).unwrap();
            let saved = text.lines().find(|l| l.starts_with("saved: ")).unwrap();
            let later = format!("saved: 2020-01-0{}T00:00:00Z", n + 1);
            fs::write(&path, text.replace(saved, &later)).unwrap();
            ids.push(id);
        }
        let found = store.search("tea").unwrap();
        let order: Vec<_> = found.iter().map(|f| f.bookmark.id.clone()).collect();
        assert_eq!(order, [&ids[1], &ids[2], &ids[0]].map(Clone::clone));
        let snippet = &found[0].snippet;
        assert_eq!(
            (snippet.text.as_str(), snippet.matched.clone()),
            ("About", 0..0)
        );
        assert_eq!(found[1].snippet.text, "tea");
    }

    #[test]
    fn a_search_snippet_marks_the_match_in_utf16_units_after_an_emoji() {
        let scratch = Scratch::new();
        let store = scratch.store();
        saved_with_article(&store, "https://a.test/x", "T", "😀 tea\n");
        assert_eq!(store.search("tea").unwrap()[0].snippet.matched, 3..6);
    }

    #[test]
    fn article_search_gives_ranges_in_the_offsets_of_the_stored_article() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let b = saved_with_article(&store, "https://a.test/x", "T", "😀 **Tea** and tea\n");
        let article = store.read_article(&b.id).unwrap();
        let units: Vec<u16> = article.encode_utf16().collect();
        let ranges = store.search_article(&b.id, "TEA").unwrap();
        assert_eq!(ranges, [5..8, 15..18]);
        assert_eq!(
            String::from_utf16(&units[ranges[0].clone()]).unwrap(),
            "Tea"
        );
        assert!(store.search_article(&b.id, " ").unwrap().is_empty());
        let plain = saved_with_article(&store, "https://a.test/y", "T", "");
        assert!(store.search_article(&plain.id, "tea").is_err());
    }

    /// Makes Bookmark `id` a Bookmark of `address` Saved at `saved`: what two
    /// Devices Saving the same page before they Synced leave behind.
    fn resaved(scratch: &Scratch, id: &str, address: &str, saved: &str) {
        let path = scratch.0.join(format!("{id}.md"));
        let mut file = Document::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        file.set_scalar(ADDRESS, address);
        file.set_scalar(SAVED, saved);
        fs::write(path, file.render()).unwrap();
    }

    #[test]
    fn the_same_address_saved_twice_merges_into_the_older_with_tags_united() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let older = plain(&store, "https://a.example/");
        let newer = plain(&store, "https://b.example/");
        store.add_tag(&older.id, "read").unwrap();
        store.add_tag(&newer.id, "later").unwrap();
        resaved(
            &scratch,
            &older.id,
            "https://a.example/",
            "2024-01-01T00:00:00Z",
        );
        resaved(
            &scratch,
            &newer.id,
            "https://a.example/",
            "2024-01-02T00:00:00Z",
        );

        let mut touched = store.settle(&[]).unwrap();
        touched.sort();

        assert_eq!(touched.len(), 2);
        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, older.id);
        assert_eq!(all[0].tags, ["read", "later"]);
        assert_eq!(store.settle(&[]).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn settling_removes_the_states_of_changed_bookmarks_that_are_gone_or_have_no_article() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let read = saved_with_article(&store, "https://a.example/", "T", "alpha");
        store.save_progress(&read.id, 1).unwrap();
        let plain = plain(&store, "https://b.example/");
        let state = |id: &str| scratch.0.join(".state").join(format!("{id}.json"));
        let (gone, ahead) = ("zzzzzzzzzzzz", "yyyyyyyyyyyy");
        for id in [plain.id.as_str(), gone, ahead] {
            fs::write(state(id), r#"{"markup":{}}"#).unwrap();
        }

        let changed = [&read.id, &plain.id, gone, "abc.png"].map(str::to_owned);
        let mut touched = store.settle(&changed).unwrap();
        touched.sort();

        assert_eq!(touched, [plain.id.as_str(), gone]);
        assert!(state(&read.id).exists());
        assert!(!state(&plain.id).exists() && !state(gone).exists());
        assert!(
            state(ahead).exists(),
            "a state may arrive before its Bookmark"
        );
    }

    #[test]
    fn a_merge_keeps_the_article_of_whichever_has_one_with_its_markup_and_images() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let older = plain(&store, "https://a.example/");
        let images = downloaded(&[("https://i.test/a.png", PNG)]);
        let article = "![](https://i.test/a.png)\n\nalpha beta";
        let newer = store
            .save(
                "https://b.example/",
                &details("T"),
                "",
                &[],
                Some(article),
                &images,
            )
            .unwrap()
            .bookmark;
        let reading = store.read_article(&newer.id).unwrap();
        let start = reading.find("alpha").unwrap() as u64;
        highlight_at(&store, &newer.id, start, start + 5, "alpha");
        store.save_progress(&newer.id, 3).unwrap();
        resaved(
            &scratch,
            &older.id,
            "https://a.example/",
            "2024-01-01T00:00:00Z",
        );
        resaved(
            &scratch,
            &newer.id,
            "https://a.example/",
            "2024-01-02T00:00:00Z",
        );

        store.settle(&[]).unwrap();

        let merged = store.read(&older.id).unwrap();
        assert!(merged.has_article);
        assert_eq!(store.markup(&older.id).unwrap().highlights.len(), 1);
        assert_eq!(merged.progress.unwrap().offset, 3);
        assert_eq!(store.read_article(&older.id).unwrap(), reading);
        let kept = scratch.files();
        assert_eq!(
            kept.iter().filter(|f| f.ends_with(".png")).count(),
            1,
            "{kept:?}"
        );
        assert_eq!(
            kept.iter().filter(|f| f.ends_with(".md")).count(),
            1,
            "{kept:?}"
        );
    }

    #[test]
    fn a_merge_with_two_articles_keeps_the_olders_and_finds_the_others_highlights_by_text() {
        let scratch = Scratch::new();
        let store = scratch.store();
        let older = saved_with_article(&store, "https://a.example/", "T", "alpha beta");
        let newer = saved_with_article(&store, "https://b.example/", "T", "intro\n\nalpha beta");
        let start = store
            .read_article(&newer.id)
            .unwrap()
            .find("alpha")
            .unwrap() as u64;
        highlight_at(&store, &newer.id, start, start + 5, "alpha");
        resaved(
            &scratch,
            &older.id,
            "https://a.example/",
            "2024-01-01T00:00:00Z",
        );
        resaved(
            &scratch,
            &newer.id,
            "https://a.example/",
            "2024-01-02T00:00:00Z",
        );

        store.settle(&[]).unwrap();

        assert_eq!(store.read_article(&older.id).unwrap(), "alpha beta\n");
        let markup = store.markup(&older.id).unwrap();
        assert_eq!(markup.highlights[0].anchor, Anchor { start: 0, end: 5 });
        assert_eq!(store.list().unwrap().len(), 1);
    }
}
