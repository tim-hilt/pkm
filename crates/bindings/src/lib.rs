//! UniFFI surface of the core (ADR-0007). Foreign clients see exactly what is
//! exported here; the logic itself lives in `pkm-core`.

use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};

use pkm_cloud::{
    ConnectError, DirStorage, GoogleDriveStorage, Observer, OneDriveStorage, Progress, Status,
    SyncLoop, SyncState, Timing, WebDavStorage,
};
use pkm_core::scene::{FillPattern, LineStyle};
use pkm_core::{
    Command, ContextMenu, Cursor, Edges, ExportScope, FontSize, ImageFile, MagnifyEvent, Modifiers,
    PencilDoubleTap, PointerButton, PointerEvent, PointerKind, PointerPhase, Properties, Property,
    ScrollEvent, Sections, Shortcut, Sloppiness, StrokeWidth, TextAlign, TextEditor, Tool, ZOrder,
};

uniffi::setup_scaffolding!("pkmCore");

// Cloud Backing (ADR-0012): the Cloud Providers live in their own crate.

/// Why Connecting to a Cloud Provider failed; the client words the message.
#[uniffi::remote(Error)]
pub enum ConnectError {
    WrongCredentials,
    Unreachable,
    NotHttps,
    NotWebDav,
    ICloudDriveUnavailable,
    WrongFolder,
    AccessDenied,
    SignInFailed,
    UnsupportedAccount,
}

/// Connects to the WebDAV server at `url` as `username` with `password`, which
/// checks the server and creates the `pkm` folder under `url` if
/// it is missing. Blocks while it talks to the server, so call it off the main
/// thread. Keeps nothing: the client keeps the credentials.
#[uniffi::export]
pub fn connect_webdav(url: String, username: String, password: String) -> Result<(), ConnectError> {
    WebDavStorage::connect(&url, &username, &password).map(drop)
}

/// Connects to iCloud Drive through `granted_directory`, the folder the user
/// picked: it must be iCloud Drive's root. Creates `pkm` in it if
/// missing and returns its path, which `CloudSync::start_icloud_drive` Syncs
/// with. The client must have access to the folder while this and Sync run.
#[uniffi::export]
pub fn connect_icloud_drive(granted_directory: String) -> Result<String, ConnectError> {
    pkm_cloud::icloud::connect(granted_directory.as_ref())
        .map(|folder| folder.to_string_lossy().into_owned())
}

/// A Google Drive sign-in under way. The client shows `url` in the system's
/// web authentication session, listening for `callback_scheme`, and hands the
/// redirect it ends with to `finish`. If the user cancels the page, drop this
/// and do nothing else.
#[derive(uniffi::Object)]
pub struct GoogleDriveSignIn {
    sign_in: Mutex<Option<pkm_cloud::gdrive::SignIn>>,
    url: String,
    callback_scheme: String,
}

/// The account signed in to.
#[derive(Debug, uniffi::Record)]
pub struct GoogleDriveAccount {
    /// To keep in the keychain; Sync resumes from it.
    pub refresh_token: String,
    /// For the user to see.
    pub email: String,
}

#[uniffi::export]
impl GoogleDriveSignIn {
    /// Starts signing in with the app's Google OAuth client `client_id`.
    #[uniffi::constructor]
    pub fn start(client_id: String) -> Result<Arc<Self>, ConnectError> {
        let sign_in = pkm_cloud::gdrive::SignIn::start(&client_id)?;
        Ok(Arc::new(Self {
            url: sign_in.url().to_owned(),
            callback_scheme: sign_in.callback_scheme(),
            sign_in: Mutex::new(Some(sign_in)),
        }))
    }

    /// The sign-in page.
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// The URL scheme the page redirects to.
    pub fn callback_scheme(&self) -> String {
        self.callback_scheme.clone()
    }

    /// Finishes with the `redirect` URL the page ended at: creates
    /// `My Drive/pkm` if missing. Refused consent is
    /// `AccessDenied`. Blocks while it talks to Google, so call it off the
    /// main thread. Keeps nothing: the client keeps the refresh token.
    pub fn finish(&self, redirect: String) -> Result<GoogleDriveAccount, ConnectError> {
        let sign_in = self
            .sign_in
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or(ConnectError::SignInFailed)?;
        let account = sign_in.finish(&redirect)?;
        Ok(GoogleDriveAccount {
            refresh_token: account.refresh_token,
            email: account.email,
        })
    }
}

/// A OneDrive sign-in under way. The client shows `url` in the system's
/// web authentication session, listening for `callback_scheme`, and hands the
/// redirect it ends with to `finish`. If the user cancels the page, drop this
/// and do nothing else.
#[derive(uniffi::Object)]
pub struct OneDriveSignIn {
    sign_in: Mutex<Option<pkm_cloud::onedrive::SignIn>>,
    url: String,
    callback_scheme: String,
}

/// The account signed in to.
#[derive(Debug, uniffi::Record)]
pub struct OneDriveAccount {
    /// To keep in the keychain; Sync resumes from it.
    pub refresh_token: String,
    /// For the user to see.
    pub email: String,
}

#[uniffi::export]
impl OneDriveSignIn {
    /// Starts signing in with the app's Microsoft OAuth client `client_id`.
    #[uniffi::constructor]
    pub fn start(client_id: String) -> Result<Arc<Self>, ConnectError> {
        let sign_in = pkm_cloud::onedrive::SignIn::start(&client_id)?;
        Ok(Arc::new(Self {
            url: sign_in.url().to_owned(),
            callback_scheme: sign_in.callback_scheme(),
            sign_in: Mutex::new(Some(sign_in)),
        }))
    }

    /// The sign-in page.
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// The URL scheme the page redirects to.
    pub fn callback_scheme(&self) -> String {
        self.callback_scheme.clone()
    }

    /// Finishes with the `redirect` URL the page ended at: creates
    /// `Apps/pkm`. Refused consent is
    /// `AccessDenied`, a work or school account `UnsupportedAccount`. Blocks while it talks to Microsoft, so call it off the
    /// main thread. Keeps nothing: the client keeps the refresh token.
    pub fn finish(&self, redirect: String) -> Result<OneDriveAccount, ConnectError> {
        let sign_in = self
            .sign_in
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
            .ok_or(ConnectError::SignInFailed)?;
        let account = sign_in.finish(&redirect)?;
        Ok(OneDriveAccount {
            refresh_token: account.refresh_token,
            email: account.email,
        })
    }
}

/// Hears of a refresh token that replaced the one Sync was started with.
#[uniffi::export(with_foreign)]
pub trait TokenObserver: Send + Sync {
    /// Keep `token` in the keychain instead of the old one. Called on a
    /// thread of the Sync loop.
    fn refresh_token_rotated(&self, token: String);
}

fn google_drive_storage(
    client_id: &str,
    refresh_token: &str,
    tokens: Arc<dyn TokenObserver>,
) -> Arc<GoogleDriveStorage> {
    Arc::new(GoogleDriveStorage::resume(
        client_id,
        refresh_token,
        move |token| tokens.refresh_token_rotated(token.to_owned()),
    ))
}

fn one_drive_storage(
    client_id: &str,
    refresh_token: &str,
    tokens: Arc<dyn TokenObserver>,
) -> Arc<OneDriveStorage> {
    Arc::new(OneDriveStorage::resume(
        client_id,
        refresh_token,
        move |token| tokens.refresh_token_rotated(token.to_owned()),
    ))
}

/// Asks the system to download files that are in iCloud Drive only.
#[uniffi::export(with_foreign)]
pub trait Downloader: Send + Sync {
    /// Start downloading the file at `path`, which does not exist on the
    /// Device yet. Must not block.
    fn download(&self, path: String);
}

fn icloud_storage(
    folder: &str,
    downloader: Arc<dyn Downloader>,
) -> Result<Arc<DirStorage>, SyncStartError> {
    DirStorage::open_downloading(folder, move |path| {
        downloader.download(path.to_string_lossy().into_owned());
    })
    .map(Arc::new)
    // The grant is gone: the status then says so.
    .map_err(|_| SyncStartError::AccessLost)
}

/// Hears what Sync did to the Drawings folder.
#[uniffi::export(with_foreign)]
pub trait SyncObserver: Send + Sync {
    /// Sync wrote or deleted the files of these Drawings (names without
    /// `.excalidraw`; a rename is the old name and the new one). Called on a
    /// thread of the Sync loop, not the main thread.
    fn changed(&self, names: Vec<String>);

    /// Drawings are being transferred: `done` of `total`. `total` is 0 once
    /// the step is over. Also on the Sync loop's thread.
    fn progress(&self, done: u32, total: u32);

    /// The status changed.
    fn status(&self, status: SyncStatus);
}

/// What Cloud Backing is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum SyncStatus {
    UpToDate,
    Syncing,
    Offline,
    /// The cloud rejected the credentials: Reconnect.
    AccessLost,
    /// The cloud is full: nothing is uploaded, downloads go on.
    Full,
}

impl From<Status> for SyncStatus {
    fn from(status: Status) -> Self {
        match status {
            Status::UpToDate => Self::UpToDate,
            Status::Syncing => Self::Syncing,
            Status::Offline => Self::Offline,
            Status::AccessLost => Self::AccessLost,
            Status::Full => Self::Full,
        }
    }
}

/// Hands what the loop does to the foreign observer.
struct ForeignObserver(Arc<dyn SyncObserver>);

impl Observer for ForeignObserver {
    fn changed(&self, names: Vec<String>) {
        self.0.changed(names);
    }
    fn progress(&self, progress: Option<Progress>) {
        let (done, total) = progress.map_or((0, 0), |p| (p.done, p.total));
        self.0.progress(done, total);
    }
    fn status(&self, status: Status) {
        self.0.status(status.into());
    }
}

/// Why Sync could not start.
#[derive(Debug, uniffi::Error)]
pub enum SyncStartError {
    /// The WebDAV URL doesn't start with `https://`.
    NotHttps,
    /// The folder for the Sync state could not be made.
    StateUnavailable,
    /// iCloud Drive's folder cannot be opened: the grant is lost.
    AccessLost,
}

impl fmt::Display for SyncStartError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Sync between the Drawings folder and a Cloud Provider, running until this
/// object is released. Releasing it cancels a step that is running. Nothing
/// runs while the app is in the background.
#[derive(uniffi::Object)]
pub struct CloudSync {
    sync_loop: SyncLoop,
}

#[uniffi::export]
impl CloudSync {
    /// Starts Syncing the Drawings in `drawings_directory` with the WebDAV
    /// server, remembering what was Synced in `state_directory`, which must be
    /// outside the Drawings folder. Contacts nobody before returning, so it
    /// works offline; the first step runs at once.
    #[uniffi::constructor]
    pub fn start_webdav(
        url: String,
        username: String,
        password: String,
        drawings_directory: String,
        state_directory: String,
        observer: Arc<dyn SyncObserver>,
    ) -> Result<Arc<Self>, SyncStartError> {
        let storage = WebDavStorage::resume(&url, &username, &password)
            .map_err(|_| SyncStartError::NotHttps)?;
        let state =
            SyncState::open(state_directory).map_err(|_| SyncStartError::StateUnavailable)?;
        let sync_loop = SyncLoop::start(
            drawings_directory.into(),
            state,
            Arc::new(storage),
            Timing::default(),
            Arc::new(ForeignObserver(observer)),
        );
        Ok(Arc::new(Self { sync_loop }))
    }

    /// Starts Syncing like `start_webdav`, with the `pkm` folder
    /// that `connect_icloud_drive` returned. The client must have access to it
    /// (security scope) as long as this object lives.
    #[uniffi::constructor]
    pub fn start_icloud_drive(
        folder: String,
        downloader: Arc<dyn Downloader>,
        drawings_directory: String,
        state_directory: String,
        observer: Arc<dyn SyncObserver>,
    ) -> Result<Arc<Self>, SyncStartError> {
        let storage = icloud_storage(&folder, downloader)?;
        let state =
            SyncState::open(state_directory).map_err(|_| SyncStartError::StateUnavailable)?;
        let sync_loop = SyncLoop::start(
            drawings_directory.into(),
            state,
            storage,
            Timing::default(),
            Arc::new(ForeignObserver(observer)),
        );
        Ok(Arc::new(Self { sync_loop }))
    }

    /// Starts Syncing like `start_webdav`, with `My Drive/pkm` of
    /// the Google account that `GoogleDriveSignIn::finish` returned the
    /// `refresh_token` of.
    #[uniffi::constructor]
    pub fn start_google_drive(
        client_id: String,
        refresh_token: String,
        tokens: Arc<dyn TokenObserver>,
        drawings_directory: String,
        state_directory: String,
        observer: Arc<dyn SyncObserver>,
    ) -> Result<Arc<Self>, SyncStartError> {
        let storage = google_drive_storage(&client_id, &refresh_token, tokens);
        let state =
            SyncState::open(state_directory).map_err(|_| SyncStartError::StateUnavailable)?;
        let sync_loop = SyncLoop::start(
            drawings_directory.into(),
            state,
            storage,
            Timing::default(),
            Arc::new(ForeignObserver(observer)),
        );
        Ok(Arc::new(Self { sync_loop }))
    }

    /// Starts Syncing like `start_webdav`, with `Apps/pkm` of the
    /// OneDrive account that `OneDriveSignIn::finish` returned the
    /// `refresh_token` of.
    #[uniffi::constructor]
    pub fn start_one_drive(
        client_id: String,
        refresh_token: String,
        tokens: Arc<dyn TokenObserver>,
        drawings_directory: String,
        state_directory: String,
        observer: Arc<dyn SyncObserver>,
    ) -> Result<Arc<Self>, SyncStartError> {
        let storage = one_drive_storage(&client_id, &refresh_token, tokens);
        let state =
            SyncState::open(state_directory).map_err(|_| SyncStartError::StateUnavailable)?;
        let sync_loop = SyncLoop::start(
            drawings_directory.into(),
            state,
            storage,
            Timing::default(),
            Arc::new(ForeignObserver(observer)),
        );
        Ok(Arc::new(Self { sync_loop }))
    }

    /// The app came to the front: Sync at once.
    pub fn came_to_front(&self) {
        self.sync_loop.came_to_front();
    }

    /// The app went to the background: nothing runs until it is back.
    pub fn went_to_background(&self) {
        self.sync_loop.went_to_background();
    }

    /// A Drawing changed on this Device: Sync a few seconds after the last
    /// change.
    pub fn local_change(&self) {
        self.sync_loop.local_change();
    }

    /// The Device lost its connection or has it back. Offline nothing runs;
    /// back online Syncs at once.
    pub fn set_online(&self, online: bool) {
        self.sync_loop.set_online(online);
    }

    /// The user Reconnected with new credentials: Sync resumes where it
    /// stopped. If the server rejects them too, the status is `AccessLost`
    /// again.
    pub fn reconnect_one_drive(
        &self,
        client_id: String,
        refresh_token: String,
        tokens: Arc<dyn TokenObserver>,
    ) {
        let storage = one_drive_storage(&client_id, &refresh_token, tokens);
        self.sync_loop.reconnect(storage);
    }

    /// Like `reconnect_one_drive`, for Google Drive.
    pub fn reconnect_google_drive(
        &self,
        client_id: String,
        refresh_token: String,
        tokens: Arc<dyn TokenObserver>,
    ) {
        let storage = google_drive_storage(&client_id, &refresh_token, tokens);
        self.sync_loop.reconnect(storage);
    }

    /// Like `reconnect_one_drive`, for WebDAV.
    pub fn reconnect_webdav(
        &self,
        url: String,
        username: String,
        password: String,
    ) -> Result<(), SyncStartError> {
        let storage = WebDavStorage::resume(&url, &username, &password)
            .map_err(|_| SyncStartError::NotHttps)?;
        self.sync_loop.reconnect(Arc::new(storage));
        Ok(())
    }
}

/// Forgets what was Synced (Disconnect). Release the `CloudSync` first.
#[uniffi::export]
pub fn forget_sync_state(state_directory: String) {
    let _ = SyncState::forget(state_directory.as_ref());
}

/// Whether two Drawings are one Drawing: they share an Element, or both have
/// none. Bytes that are not a Drawing are no Drawing's.
#[uniffi::export]
pub fn are_same_drawing(a: Vec<u8>, b: Vec<u8>) -> bool {
    pkm_core::are_same_drawing(&a, &b).unwrap_or(false)
}

/// Why a file could not be opened as a Drawing, in words fit for a user.
#[derive(Debug, uniffi::Error)]
pub enum LoadError {
    Invalid { message: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let LoadError::Invalid { message } = self;
        f.write_str(message)
    }
}

impl From<pkm_core::LoadError> for LoadError {
    fn from(error: pkm_core::LoadError) -> Self {
        LoadError::Invalid {
            message: error.to_string(),
        }
    }
}

// Input events are plain data: UniFFI mirrors the core's definitions as
// records and enums, so there is exactly one definition of each.

#[uniffi::remote(Record)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[uniffi::remote(Enum)]
pub enum ExportScope {
    Scene,
    Selection,
}

/// An image export for the client to rasterise; see
/// [`pkm_core::Export`].
#[derive(uniffi::Record)]
pub struct Export {
    pub geometry: Arc<Geometry>,
    /// The Scene rectangle the image shows, in Scene units.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[uniffi::remote(Enum)]
pub enum PointerKind {
    Finger,
    Pencil,
    Mouse,
}

#[uniffi::remote(Enum)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

#[uniffi::remote(Enum)]
pub enum PointerPhase {
    Down,
    Move,
    Up,
    Cancel,
}

#[uniffi::remote(Record)]
pub struct PointerEvent {
    pub id: u64,
    pub phase: PointerPhase,
    pub x: f64,
    pub y: f64,
    pub kind: PointerKind,
    pub button: PointerButton,
    pub pressure: f32,
    pub timestamp: f64,
    pub modifiers: Modifiers,
}

#[uniffi::remote(Record)]
pub struct ScrollEvent {
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub modifiers: Modifiers,
}

#[uniffi::remote(Record)]
pub struct MagnifyEvent {
    pub x: f64,
    pub y: f64,
    pub magnification: f64,
}

#[uniffi::remote(Enum)]
pub enum Tool {
    Selection,
    Hand,
    Rectangle,
    Diamond,
    Ellipse,
    Line,
    Arrow,
    Freedraw,
    Eraser,
    Text,
}

#[uniffi::remote(Enum)]
pub enum Cursor {
    Arrow,
    Crosshair,
    Text,
    Move,
    Grab,
    Grabbing,
    Pointer,
    Eraser,
    ResizeNorthSouth,
    ResizeEastWest,
    ResizeNorthWestSouthEast,
    ResizeNorthEastSouthWest,
}

#[uniffi::remote(Enum)]
pub enum PencilDoubleTap {
    SwitchEraser,
    SwitchPrevious,
    Ignore,
}

#[uniffi::remote(Enum)]
pub enum ZOrder {
    Forward,
    Backward,
    ToFront,
    ToBack,
}

// The properties panel's options and values; see
// [`pkm_core::Property`].

#[uniffi::remote(Enum)]
pub enum FillPattern {
    Hachure,
    CrossHatch,
    Zigzag,
    Solid,
}

#[uniffi::remote(Enum)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

#[uniffi::remote(Enum)]
pub enum StrokeWidth {
    Thin,
    Medium,
    Bold,
}

#[uniffi::remote(Enum)]
pub enum Sloppiness {
    Architect,
    Artist,
    Cartoonist,
}

#[uniffi::remote(Enum)]
pub enum Edges {
    Sharp,
    Round,
}

#[uniffi::remote(Enum)]
pub enum FontSize {
    Small,
    Medium,
    Large,
    ExtraLarge,
}

#[uniffi::remote(Enum)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[uniffi::remote(Enum)]
pub enum Property {
    StrokeColor(String),
    BackgroundColor(String),
    FillStyle(FillPattern),
    StrokeWidth(StrokeWidth),
    StrokeStyle(LineStyle),
    Sloppiness(Sloppiness),
    Edges(Edges),
    Opacity(f64),
    FontSize(FontSize),
    TextAlign(TextAlign),
}

#[uniffi::remote(Enum)]
pub enum Command {
    SetTool(Tool),
    ToggleToolLock,
    Undo,
    Redo,
    Duplicate,
    Delete,
    Group,
    Ungroup,
    MoveInZOrder(ZOrder),
    SelectAll,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ZoomToFit,
    FinishLine,
    FinishLineOrEditText,
}

#[uniffi::remote(Record)]
pub struct Shortcut {
    pub key: String,
    pub modifiers: Modifiers,
    pub command: Command,
}

/// How Excalidraw shows a CSS colour in its dark theme, for swatches; see
/// [`pkm_core::dark_mode_color`].
#[uniffi::export]
pub fn dark_mode_color(css: String) -> String {
    pkm_core::dark_mode_color(&css)
}

/// Every keyboard shortcut; see [`pkm_core::shortcuts`].
#[uniffi::export]
pub fn shortcuts() -> Vec<Shortcut> {
    pkm_core::shortcuts()
}

#[uniffi::remote(Record)]
pub struct Properties {
    pub stroke_color: Option<String>,
    pub background_color: Option<String>,
    pub fill_style: Option<FillPattern>,
    pub stroke_width: Option<StrokeWidth>,
    pub stroke_style: Option<LineStyle>,
    pub sloppiness: Option<Sloppiness>,
    pub edges: Option<Edges>,
    pub opacity: Option<f64>,
    pub font_size: Option<FontSize>,
    pub text_align: Option<TextAlign>,
    pub sections: Sections,
}

#[uniffi::remote(Record)]
pub struct Sections {
    pub stroke_color: bool,
    pub background_color: bool,
    pub fill_style: bool,
    pub stroke_width: bool,
    pub stroke_style: bool,
    pub sloppiness: bool,
    pub edges: bool,
    pub opacity: bool,
    pub text: bool,
}

/// The text view to show over the Canvas while a text is edited, in view
/// coordinates; see [`pkm_core::TextEditor`].
#[uniffi::remote(Record)]
pub struct TextEditor {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub line_height: f64,
    pub font_family: u32,
    pub align: TextAlign,
    pub color: u32,
    pub opacity: f64,
    pub angle: f64,
    pub zoom: f64,
    pub fixed_width: bool,
}

/// Which menu a long-press opens; see [`pkm_core::ContextMenu`].
#[uniffi::remote(Enum)]
pub enum ContextMenu {
    Element { can_group: bool, can_ungroup: bool },
    Canvas,
}

/// An image to paste or drop; see [`pkm_core::ImageFile`].
#[uniffi::remote(Record)]
pub struct ImageFile {
    pub file_id: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// What the Canvas shows; see [`pkm_core::Viewport`]. Small and
/// cheap enough to read after every input event.
#[derive(uniffi::Record)]
pub struct ViewportState {
    /// Scene point at the Canvas's top-left corner.
    pub origin_x: f64,
    pub origin_y: f64,
    /// View points per Scene unit.
    pub zoom: f64,
}

impl From<pkm_core::Viewport> for ViewportState {
    fn from(viewport: pkm_core::Viewport) -> Self {
        let (origin_x, origin_y) = viewport.origin();
        Self {
            origin_x,
            origin_y,
            zoom: viewport.zoom(),
        }
    }
}

/// Measures text for the core; implemented by clients with their font
/// machinery. See [`pkm_core::TextMeasurer`].
#[uniffi::export(with_foreign)]
pub trait TextMeasurer: Send + Sync {
    /// The advance width in Scene units of `text`, a single line, set in
    /// Excalifont at `font_size`.
    fn line_width(&self, text: String, font_size: f64) -> f64;
}

/// Lets the core call a client's measurer.
struct Measurer(Arc<dyn TextMeasurer>);

impl pkm_core::TextMeasurer for Measurer {
    fn line_width(&self, text: &str, font_size: f64) -> f64 {
        self.0.line_width(text.to_owned(), font_size)
    }
}

/// Handle to the core's editor; see [`pkm_core::Editor`].
///
/// UniFFI objects are shared, so the editor sits behind a lock; clients drive
/// it from one thread and never contend for it.
#[derive(uniffi::Object)]
pub struct Editor {
    inner: Mutex<pkm_core::Editor>,
}

impl Editor {
    fn editor(&self) -> MutexGuard<'_, pkm_core::Editor> {
        // A panic while holding the lock cannot leave the editor half-edited
        // in a way that matters more than carrying on does.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[uniffi::export]
impl Editor {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(pkm_core::Editor::new()),
        }
    }

    /// Replaces the Scene with the Drawing in `bytes` (a `.excalidraw` file's
    /// content); see [`pkm_core::Editor::load`].
    pub fn load(&self, bytes: Vec<u8>) -> Result<(), LoadError> {
        Ok(self.editor().load(&bytes)?)
    }

    /// The Scene as `.excalidraw` file content.
    pub fn save(&self) -> Vec<u8> {
        self.editor().save()
    }

    /// Merges `bytes`, the Drawing as Sync rewrote its file, into the open
    /// Scene without reloading; see
    /// [`pkm_core::Editor::merge_synced`].
    pub fn merge_synced(&self, bytes: Vec<u8>) -> Result<(), LoadError> {
        self.editor()
            .merge_synced(&bytes)
            .map_err(|e| LoadError::Invalid {
                message: e.to_string(),
            })
    }

    /// Number of Elements shown in the Scene.
    pub fn element_count(&self) -> u64 {
        self.editor().element_count()
    }

    /// Hands the core the client's way to measure text; see
    /// [`pkm_core::Editor::set_text_measurer`]. Call it before
    /// reading geometry that has labels in it.
    pub fn set_text_measurer(&self, measurer: Arc<dyn TextMeasurer>) {
        self.editor()
            .set_text_measurer(Arc::new(Measurer(measurer)));
    }

    /// The part of the Scene the Canvas shows. Read it after forwarding
    /// input; it never changes the geometry, which clients keep.
    pub fn viewport(&self) -> ViewportState {
        self.editor().viewport().into()
    }

    /// Shows the Scene from the given viewport, e.g. the one a Drawing was
    /// left at; see [`pkm_core::Editor::set_viewport`]. Not a change
    /// of the Scene.
    pub fn set_viewport(&self, viewport: ViewportState) {
        self.editor()
            .set_viewport((viewport.origin_x, viewport.origin_y), viewport.zoom);
    }

    /// Reports the Canvas size in points; call on start and on every resize.
    pub fn set_view_size(&self, width: f64, height: f64) {
        self.editor().set_view_size(width, height);
    }

    /// One zoom step in around the middle of the view.
    pub fn zoom_in(&self) {
        self.editor().zoom_in();
    }

    /// One zoom step out around the middle of the view.
    pub fn zoom_out(&self) {
        self.editor().zoom_out();
    }

    /// Back to 100 % around the middle of the view.
    pub fn reset_zoom(&self) {
        self.editor().reset_zoom();
    }

    /// Frames all shown Elements.
    pub fn zoom_to_fit(&self) {
        self.editor().zoom_to_fit();
    }

    /// The active Tool.
    pub fn tool(&self) -> Tool {
        self.editor().tool()
    }

    pub fn set_tool(&self, tool: Tool) {
        self.editor().set_tool(tool);
    }

    /// The Pencil was double-tapped; `action` is what the user set for it in
    /// the system settings (anything that is not about Tools is `Ignore`).
    /// Read the Tool afterwards.
    pub fn pencil_double_tap(&self, action: PencilDoubleTap) {
        self.editor().pencil_double_tap(action);
    }

    /// Whether the Tool lock is on: the Tool stays after drawing a shape.
    pub fn tool_lock(&self) -> bool {
        self.editor().tool_lock()
    }

    /// Shows colours dark (the system appearance); see
    /// [`pkm_core::Editor::set_dark_mode`]. It does not change
    /// `revision`: redraw after calling it.
    pub fn set_dark_mode(&self, on: bool) {
        self.editor().set_dark_mode(on);
    }

    /// Whether colours are shown dark.
    pub fn dark_mode(&self) -> bool {
        self.editor().dark_mode()
    }

    /// The colour of the Canvas as `0xRRGGBBAA`, as displayed; see
    /// [`pkm_core::Editor::canvas_color`].
    pub fn canvas_color(&self) -> u32 {
        self.editor().canvas_color()
    }

    /// Turns the Tool lock on or off; turning it off returns to selection.
    pub fn set_tool_lock(&self, locked: bool) {
        self.editor().set_tool_lock(locked);
    }

    /// Whether Pencil-only drawing is on: the Pencil uses the current Tool
    /// while fingers only select, pan and zoom. It turns on by itself with
    /// the first Pencil input unless [`Editor::set_pencil_only_drawing`] was
    /// called, so read it after forwarding input and store it when it changes.
    pub fn pencil_only_drawing(&self) -> bool {
        self.editor().pencil_only_drawing()
    }

    /// The user's choice in settings, or the stored setting restored at
    /// launch (call it only if one is stored). Final: a Pencil no longer turns
    /// it on by itself afterwards.
    pub fn set_pencil_only_drawing(&self, on: bool) {
        self.editor().set_pencil_only_drawing(on);
    }

    /// Differs from before whenever the Scene changed; read it after
    /// forwarding input and ask for [`Editor::geometry`] again if it moved.
    pub fn revision(&self) -> u64 {
        self.editor().revision()
    }

    /// Whether there is something to undo; the undo button is enabled
    /// exactly then. Read it after forwarding input or a command.
    pub fn can_undo(&self) -> bool {
        self.editor().can_undo()
    }

    /// Whether there is something to redo; see [`Editor::can_undo`].
    pub fn can_redo(&self) -> bool {
        self.editor().can_redo()
    }

    /// Takes back the latest change to the Scene; see
    /// [`pkm_core::Editor::undo`].
    pub fn undo(&self) {
        self.editor().undo();
    }

    /// Does again what was last undone.
    pub fn redo(&self) {
        self.editor().redo();
    }

    /// The ids of the selected Elements, in the order they were selected.
    pub fn selection(&self) -> Vec<String> {
        self.editor().selection().to_vec()
    }

    /// Whether grouping the Selection would do anything; the Element menu and
    /// the Canvas's accessibility custom actions offer Group exactly then.
    pub fn can_group(&self) -> bool {
        self.editor().can_group()
    }

    /// Whether ungrouping the Selection would do anything; see
    /// [`Editor::can_group`].
    pub fn can_ungroup(&self) -> bool {
        self.editor().can_ungroup()
    }

    /// What the properties panel shows; see
    /// [`pkm_core::Editor::properties`]. Read it after forwarding
    /// input or a command.
    pub fn properties(&self) -> Properties {
        self.editor().properties()
    }

    /// Whether the properties panel is shown; see
    /// [`pkm_core::Editor::shows_properties`]. Read it with
    /// [`Editor::properties`].
    pub fn shows_properties(&self) -> bool {
        self.editor().shows_properties()
    }

    /// Sets a property on the Selection and as the style of the next Element;
    /// see [`pkm_core::Editor::set_property`].
    pub fn set_property(&self, property: Property) {
        self.editor().set_property(property);
    }

    /// The text view to show over the Canvas, `None` if no text is being
    /// edited; see [`pkm_core::Editor::text_editor`]. Read it with
    /// the viewport after forwarding input or a command.
    pub fn text_editor(&self) -> Option<TextEditor> {
        self.editor().text_editor()
    }

    /// Tells the core what the text view says now; see
    /// [`pkm_core::Editor::set_editing_text`].
    pub fn set_editing_text(&self, text: String) {
        self.editor().set_editing_text(&text);
    }

    /// Ends the editing of the text and keeps it; see
    /// [`pkm_core::Editor::commit_text`].
    pub fn commit_text(&self) {
        self.editor().commit_text();
    }

    /// Opens the text view on the text of the Selection (Enter on a Mac):
    /// the label of a selected shape, which is made if it has none, or a
    /// selected text; see [`pkm_core::Editor::edit_selected_text`].
    pub fn edit_selected_text(&self) {
        self.editor().edit_selected_text();
    }

    /// Ends the editing of the text and keeps it as it was; see
    /// [`pkm_core::Editor::cancel_text`].
    pub fn cancel_text(&self) {
        self.editor().cancel_text();
    }

    /// Duplicates the Selection and selects the copies; see
    /// [`pkm_core::Editor::duplicate`].
    pub fn duplicate(&self) {
        self.editor().duplicate();
    }

    /// The text to put on the clipboard for copying the Selection, `None`
    /// with nothing selected; see
    /// [`pkm_core::Editor::copy_selection`].
    pub fn copy_selection(&self) -> Option<String> {
        self.editor().copy_selection()
    }

    /// Copies the Selection and deletes it; see
    /// [`pkm_core::Editor::cut_selection`].
    pub fn cut_selection(&self) -> Option<String> {
        self.editor().cut_selection()
    }

    /// Pastes Excalidraw clipboard JSON in the middle of the view, returns
    /// whether `text` was one; see [`pkm_core::Editor::paste`].
    pub fn paste(&self, text: String) -> bool {
        self.editor().paste(&text)
    }

    /// Pastes Excalidraw clipboard JSON around the point `(x, y)` of the
    /// view; see [`pkm_core::Editor::paste_at`].
    pub fn paste_at(&self, text: String, x: f64, y: f64) -> bool {
        self.editor().paste_at(&text, x, y)
    }

    /// Puts pasted images in the middle of the view; see
    /// [`pkm_core::Editor::insert_images`].
    pub fn insert_images(&self, images: Vec<ImageFile>) {
        self.editor().insert_images(images);
    }

    /// Puts dropped images around the point `(x, y)` of the view; see
    /// [`pkm_core::Editor::insert_images_at`].
    pub fn insert_images_at(&self, images: Vec<ImageFile>, x: f64, y: f64) {
        self.editor().insert_images_at(images, x, y);
    }

    /// Deletes the Selection; see
    /// [`pkm_core::Editor::delete_selection`].
    pub fn delete_selection(&self) {
        self.editor().delete_selection();
    }

    /// Groups the Selection; see [`pkm_core::Editor::group`].
    pub fn group(&self) {
        self.editor().group();
    }

    /// Dissolves the groups that are selected; see
    /// [`pkm_core::Editor::ungroup`].
    pub fn ungroup(&self) {
        self.editor().ungroup();
    }

    /// Carries out `command`; see [`pkm_core::Editor::perform`].
    pub fn perform(&self, command: Command) {
        self.editor().perform(command);
    }

    /// Moves the Selection in z-order; see
    /// [`pkm_core::Editor::move_in_z_order`].
    pub fn move_in_z_order(&self, how: ZOrder) {
        self.editor().move_in_z_order(how);
    }

    /// Differs from before whenever [`Editor::overlay`] may: read it after
    /// forwarding input and ask for the overlay again if it moved.
    pub fn overlay_revision(&self) -> u64 {
        self.editor().overlay_revision()
    }

    /// The selection frames to draw over the Scene, in Scene coordinates and
    /// for the current zoom; see [`pkm_core::Editor::overlay`].
    pub fn overlay(&self) -> Arc<Geometry> {
        Arc::new(Geometry {
            inner: self.editor().overlay(),
        })
    }

    /// A step of a finger, Pencil or mouse; see
    /// [`pkm_core::Editor::pointer`].
    pub fn pointer(&self, event: PointerEvent) {
        self.editor().pointer(event);
    }

    /// A finger or mouse held at a view point; see
    /// [`pkm_core::Editor::long_press`]. `None` if the press
    /// means nothing.
    pub fn long_press(&self, x: f64, y: f64, kind: PointerKind) -> Option<ContextMenu> {
        self.editor().long_press(x, y, kind)
    }

    /// The mouse cursor at the view point `(x, y)`; see
    /// [`pkm_core::Editor::cursor`].
    pub fn cursor(&self, x: f64, y: f64) -> Cursor {
        self.editor().cursor(x, y)
    }

    /// The space bar went down (`true`) or up; see
    /// [`pkm_core::Editor::set_pan_key`].
    pub fn set_pan_key(&self, held: bool) {
        self.editor().set_pan_key(held);
    }

    /// A scroll wheel or trackpad scroll (Mac).
    pub fn scroll(&self, event: ScrollEvent) {
        self.editor().scroll(event);
    }

    /// A trackpad pinch (Mac).
    pub fn magnify(&self, event: MagnifyEvent) {
        self.editor().magnify(event);
    }

    /// The bytes of the image file `id` as stored in the Drawing, for the
    /// client to decode; `None` if the Drawing has no such file. Read it
    /// when [`Geometry`] names a file the client has no texture for yet.
    pub fn file_bytes(&self, id: String) -> Option<Vec<u8>> {
        self.editor().file_bytes(&id)
    }

    /// What a PNG export of `scope` shows, `None` if nothing; see
    /// [`pkm_core::Editor::export`].
    pub fn export(&self, scope: ExportScope) -> Option<Export> {
        let export = self.editor().export(scope)?;
        Some(Export {
            geometry: Arc::new(Geometry {
                inner: export.geometry,
            }),
            x: export.x,
            y: export.y,
            width: export.width,
            height: export.height,
        })
    }

    /// The paths to draw for the Scene; see [`pkm_core::Editor::geometry`].
    pub fn geometry(&self) -> Arc<Geometry> {
        Arc::new(Geometry {
            inner: self.editor().geometry(),
        })
    }
}

/// Zero-copy geometry (ADR-0007): the format is documented in
/// [`pkm_core::geometry`].
///
/// Only sizes and buffer addresses cross UniFFI; the data itself stays where
/// it is and the client reads it in place. An address is valid exactly as long
/// as the client holds this object, and the buffers never change.
#[derive(uniffi::Object)]
pub struct Geometry {
    inner: pkm_core::Geometry,
}

#[uniffi::export]
impl Geometry {
    /// Tells whether cached per-Element results are still valid; see
    /// [`pkm_core::Geometry::generation`].
    pub fn generation(&self) -> u64 {
        self.inner.generation()
    }

    /// Number of [`pkm_core::Part`] records at `parts_address`.
    pub fn part_count(&self) -> u64 {
        self.inner.parts().len() as u64
    }

    /// Size in bytes of one part record, so clients can check that their
    /// mirror of the record has the same layout.
    pub fn part_stride(&self) -> u64 {
        size_of::<pkm_core::Part>() as u64
    }

    pub fn parts_address(&self) -> u64 {
        self.inner.parts().as_ptr() as u64
    }

    /// Number of bytes (one verb each) at `verbs_address`.
    pub fn verb_count(&self) -> u64 {
        self.inner.verbs().len() as u64
    }

    pub fn verbs_address(&self) -> u64 {
        self.inner.verbs().as_ptr() as u64
    }

    /// Number of `f32` operands at `coords_address`.
    pub fn coord_count(&self) -> u64 {
        self.inner.coords().len() as u64
    }

    pub fn coords_address(&self) -> u64 {
        self.inner.coords().as_ptr() as u64
    }

    /// Number of [`pkm_core::TextLine`] records at
    /// `text_lines_address`.
    pub fn text_line_count(&self) -> u64 {
        self.inner.text_lines().len() as u64
    }

    /// Size in bytes of one text line record; see `part_stride`.
    pub fn text_line_stride(&self) -> u64 {
        size_of::<pkm_core::TextLine>() as u64
    }

    pub fn text_lines_address(&self) -> u64 {
        self.inner.text_lines().as_ptr() as u64
    }

    /// Number of bytes (UTF-8) at `text_address`.
    pub fn text_byte_count(&self) -> u64 {
        self.inner.text().len() as u64
    }

    pub fn text_address(&self) -> u64 {
        self.inner.text().as_ptr() as u64
    }

    /// Number of [`pkm_core::Image`] records at `images_address`.
    pub fn image_count(&self) -> u64 {
        self.inner.images().len() as u64
    }

    /// Size in bytes of one image record; see `part_stride`.
    pub fn image_stride(&self) -> u64 {
        size_of::<pkm_core::Image>() as u64
    }

    pub fn images_address(&self) -> u64 {
        self.inner.images().as_ptr() as u64
    }

    /// Number of bytes (UTF-8) at `file_ids_address`.
    pub fn file_id_byte_count(&self) -> u64 {
        self.inner.file_ids().len() as u64
    }

    pub fn file_ids_address(&self) -> u64 {
        self.inner.file_ids().as_ptr() as u64
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exported_addresses_point_at_the_cores_buffers_without_copying() {
        let editor = Editor::new();
        editor
            .load(
                br#"{"type":"excalidraw","elements":[
                    {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40}]}"#
                    .to_vec(),
            )
            .unwrap();
        let geometry = editor.geometry();

        assert_eq!(geometry.part_count(), 1);
        assert_eq!(geometry.generation(), editor.geometry().generation());
        assert_eq!(geometry.part_stride(), 64);
        // Four edges, each drawn twice as a move and a cubic.
        assert_eq!(geometry.verb_count(), 16);
        assert_eq!(geometry.coord_count(), 8 * 2 + 8 * 6);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let parts = geometry.parts_address() as *const pkm_core::Part;
            assert_eq!((*parts).width, 2.0);
            let verbs = geometry.verbs_address() as *const u8;
            assert_eq!(*verbs, pkm_core::Verb::Move as u8);
            let coords = geometry.coords_address() as *const f32;
            assert_eq!(*coords, 10.0);
        }
    }

    /// Half a font size per character.
    struct Monospace;

    impl TextMeasurer for Monospace {
        fn line_width(&self, text: String, font_size: f64) -> f64 {
            text.chars().count() as f64 * font_size / 2.0
        }
    }

    #[test]
    fn text_lines_are_exposed_as_buffers_and_labels_wrap_with_the_clients_measurer() {
        let editor = Editor::new();
        editor.set_text_measurer(Arc::new(Monospace));
        editor
            .load(
                br#"{"type":"excalidraw","elements":[
                {"id":"b","type":"rectangle","x":0,"y":0,"width":100,"height":100,
                 "boundElements":[{"type":"text","id":"t"}]},
                {"id":"t","type":"text","containerId":"b","fontSize":20,"text":"ab cd ef",
                 "originalText":"aaaa bbbb cccc","width":10,"height":25}]}"#
                    .to_vec(),
            )
            .unwrap();
        let geometry = editor.geometry();

        assert_eq!(geometry.text_line_count(), 2);
        assert_eq!(geometry.text_line_stride(), 52);
        assert_eq!(geometry.text_byte_count(), "aaaa bbbbcccc".len() as u64);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let lines = geometry.text_lines_address() as *const pkm_core::TextLine;
            let second = *lines.add(1);
            let text = std::slice::from_raw_parts(
                (geometry.text_address() as *const u8).add(second.text_start as usize),
                second.text_len as usize,
            );
            assert_eq!(text, b"cccc");
            assert_eq!(second.font_size, 20.0);
        }
    }

    #[test]
    fn images_and_their_files_are_exposed_without_the_core_decoding_anything() {
        let editor = Editor::new();
        editor
            .load(
                br#"{"type":"excalidraw","elements":[
                {"id":"i","type":"image","x":1,"y":2,"width":30,"height":40,"fileId":"f1"}],
                "files":{"f1":{"dataURL":"data:image/png;base64,aGVsbG8="}}}"#
                    .to_vec(),
            )
            .unwrap();
        let geometry = editor.geometry();

        assert_eq!(geometry.image_count(), 1);
        assert_eq!(geometry.image_stride(), 76);
        assert_eq!(geometry.file_id_byte_count(), 2);
        // SAFETY: the addresses come from `geometry`, which is alive and
        // whose buffers hold at least the counts reported above.
        unsafe {
            let image = *(geometry.images_address() as *const pkm_core::Image);
            assert_eq!((image.x, image.width), (1.0, 30.0));
            let id = std::slice::from_raw_parts(
                (geometry.file_ids_address() as *const u8).add(image.file_id_start as usize),
                image.file_id_len as usize,
            );
            assert_eq!(id, b"f1");
        }
        assert_eq!(editor.file_bytes("f1".into()), Some(b"hello".to_vec()));
        assert_eq!(editor.file_bytes("other".into()), None);
    }

    #[test]
    fn properties_are_set_and_read_through_the_bindings() {
        let editor = Editor::new();
        assert_eq!(editor.properties().stroke_width, Some(StrokeWidth::Medium));
        editor.set_property(Property::StrokeWidth(StrokeWidth::Bold));
        editor.set_property(Property::StrokeColor("#e03131".into()));
        let shown = editor.properties();
        assert_eq!(shown.stroke_width, Some(StrokeWidth::Bold));
        assert_eq!(shown.stroke_color.as_deref(), Some("#e03131"));
    }

    #[test]
    fn a_text_is_created_through_the_text_editor() {
        let editor = Editor::new();
        editor.set_text_measurer(Arc::new(Monospace));
        editor.set_property(Property::FontSize(FontSize::Large));
        editor.set_tool(Tool::Text);
        for phase in [PointerPhase::Down, PointerPhase::Up] {
            editor.pointer(PointerEvent {
                id: 1,
                phase,
                x: 100.0,
                y: 200.0,
                kind: PointerKind::Finger,
                button: PointerButton::Primary,
                pressure: 0.5,
                timestamp: 1.0,
                modifiers: Modifiers {
                    shift: false,
                    ctrl: false,
                    alt: false,
                    meta: false,
                },
            });
        }
        // A line of 28 points is 35 high; the first is centred on the tap.
        let open = editor.text_editor().expect("a text editor is open");
        assert_eq!((open.x, open.y, open.font_size), (100.0, 182.5, 28.0));
        assert_eq!(open.align, TextAlign::Left);

        editor.set_editing_text("Hi".into());
        assert_eq!(editor.text_editor().map(|e| e.width), Some(28.0));
        editor.commit_text();

        assert!(editor.text_editor().is_none());
        assert_eq!(editor.element_count(), 1);
        assert_eq!(editor.properties().font_size, Some(FontSize::Large));
    }

    #[test]
    fn input_events_move_the_viewport_reported_back() {
        let editor = Editor::new();
        editor.scroll(ScrollEvent {
            x: 0.0,
            y: 0.0,
            dx: 10.0,
            dy: 20.0,
            modifiers: Modifiers::default(),
        });

        let viewport = editor.viewport();
        assert_eq!((viewport.origin_x, viewport.origin_y), (-10.0, -20.0));
        assert_eq!(viewport.zoom, 1.0);
    }

    #[test]
    fn a_viewport_set_is_the_viewport_reported_back() {
        let editor = Editor::new();
        let revision = editor.revision();

        editor.set_viewport(ViewportState {
            origin_x: 30.0,
            origin_y: -12.5,
            zoom: 2.0,
        });

        let viewport = editor.viewport();
        assert_eq!((viewport.origin_x, viewport.origin_y), (30.0, -12.5));
        assert_eq!(viewport.zoom, 2.0);
        assert_eq!(editor.revision(), revision);
    }

    #[test]
    fn zoom_commands_and_the_hand_tool_work_through_the_bindings() {
        let editor = Editor::new();
        editor.set_view_size(400.0, 200.0);
        editor.zoom_in();
        assert!((editor.viewport().zoom - 1.1).abs() < 1e-9);
        editor.reset_zoom();
        assert_eq!(editor.viewport().zoom, 1.0);

        editor.set_tool(Tool::Hand);
        assert_eq!(editor.tool(), Tool::Hand);
        let touch = |phase, x| PointerEvent {
            id: 1,
            phase,
            x,
            y: 0.0,
            kind: PointerKind::Finger,
            button: PointerButton::Primary,
            pressure: 0.5,
            timestamp: 0.0,
            modifiers: Modifiers::default(),
        };
        editor.pointer(touch(PointerPhase::Down, 10.0));
        editor.pointer(touch(PointerPhase::Move, 30.0));
        assert_eq!(editor.viewport().origin_x, -20.0);
    }

    #[test]
    fn dragging_with_a_shape_tool_draws_a_shape_and_returns_to_selection() {
        let editor = Editor::new();
        let revision = editor.revision();
        editor.set_tool(Tool::Rectangle);
        let mouse = |phase, x, y| PointerEvent {
            id: 0,
            phase,
            x,
            y,
            kind: PointerKind::Mouse,
            button: PointerButton::Primary,
            pressure: 0.5,
            timestamp: 0.0,
            modifiers: Modifiers::default(),
        };
        editor.pointer(mouse(PointerPhase::Down, 10.0, 10.0));
        editor.pointer(mouse(PointerPhase::Move, 60.0, 40.0));
        editor.pointer(mouse(PointerPhase::Up, 60.0, 40.0));

        assert_ne!(editor.revision(), revision);
        assert_eq!(editor.element_count(), 1);
        assert_eq!(editor.tool(), Tool::Selection);
        assert!(!editor.tool_lock());
        editor.set_tool(Tool::Ellipse);
        editor.set_tool_lock(true);
        assert!(editor.tool_lock());
    }

    #[test]
    fn tapping_an_element_selects_it_and_the_overlay_frames_it() {
        let editor = Editor::new();
        editor
            .load(
                br##"{"type":"excalidraw","elements":[
                    {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40,
                     "backgroundColor":"#ffc9c9"}]}"##
                    .to_vec(),
            )
            .unwrap();
        assert_eq!(editor.overlay().part_count(), 0);
        let revision = editor.overlay_revision();

        let mouse = |phase| PointerEvent {
            id: 0,
            phase,
            x: 20.0,
            y: 30.0,
            kind: PointerKind::Mouse,
            button: PointerButton::Primary,
            pressure: 0.5,
            timestamp: 0.0,
            modifiers: Modifiers::default(),
        };
        editor.pointer(mouse(PointerPhase::Down));
        editor.pointer(mouse(PointerPhase::Up));

        assert_eq!(editor.selection(), ["a"]);
        assert_ne!(editor.overlay_revision(), revision);
        // The frame and the handles of the Selection: the box 10..40 by
        // 20..60 is too small for the handles on its sides, so four corners
        // and the rotation handle are drawn, each a fill and a stroke.
        assert_eq!(editor.overlay().part_count(), 1 + 5 * 2);
    }

    #[test]
    fn the_selection_commands_work_through_the_bindings() {
        let editor = Editor::new();
        editor
            .load(
                br##"{"type":"excalidraw","elements":[
                    {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40,
                     "backgroundColor":"#ffc9c9"}]}"##
                    .to_vec(),
            )
            .unwrap();
        let mouse = |phase| PointerEvent {
            id: 0,
            phase,
            x: 20.0,
            y: 30.0,
            kind: PointerKind::Mouse,
            button: PointerButton::Primary,
            pressure: 0.5,
            timestamp: 0.0,
            modifiers: Modifiers::default(),
        };
        editor.pointer(mouse(PointerPhase::Down));
        editor.pointer(mouse(PointerPhase::Up));
        assert_eq!(editor.selection(), ["a"]);

        editor.duplicate();
        assert_eq!(editor.element_count(), 2);
        editor.move_in_z_order(ZOrder::ToBack);
        assert!(editor.can_undo());
        editor.delete_selection();
        assert_eq!(editor.element_count(), 1);
        assert!(editor.selection().is_empty());
    }

    #[test]
    fn a_long_press_passes_through_the_bindings() {
        let editor = Editor::new();
        editor
            .load(
                br##"{"type":"excalidraw","elements":[
                    {"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40,
                     "backgroundColor":"#ffc9c9"}]}"##
                    .to_vec(),
            )
            .unwrap();

        assert_eq!(
            editor.long_press(20.0, 30.0, PointerKind::Finger),
            Some(ContextMenu::Element {
                can_group: false,
                can_ungroup: false
            })
        );
        assert_eq!(editor.selection(), ["a"]);
        assert_eq!(
            editor.long_press(300.0, 300.0, PointerKind::Finger),
            Some(ContextMenu::Canvas)
        );
        assert_eq!(editor.long_press(20.0, 30.0, PointerKind::Pencil), None);
    }

    #[test]
    fn grouping_works_through_the_bindings() {
        let editor = Editor::new();
        editor
            .load(
                br##"{"type":"excalidraw","elements":[
                    {"id":"a","type":"rectangle","x":0,"y":0,"width":30,"height":40,
                     "backgroundColor":"#ffc9c9"},
                    {"id":"b","type":"rectangle","x":100,"y":0,"width":30,"height":40,
                     "backgroundColor":"#ffc9c9"}]}"##
                    .to_vec(),
            )
            .unwrap();
        let mouse = |phase, x, shift| PointerEvent {
            id: 0,
            phase,
            x,
            y: 20.0,
            kind: PointerKind::Mouse,
            button: PointerButton::Primary,
            pressure: 0.5,
            timestamp: 0.0,
            modifiers: Modifiers {
                shift,
                ..Modifiers::default()
            },
        };
        for (x, shift) in [(15.0, false), (115.0, true)] {
            editor.pointer(mouse(PointerPhase::Down, x, shift));
            editor.pointer(mouse(PointerPhase::Up, x, shift));
        }
        assert_eq!(
            editor.long_press(15.0, 20.0, PointerKind::Mouse),
            Some(ContextMenu::Element {
                can_group: true,
                can_ungroup: false
            })
        );

        editor.group();
        assert_eq!(
            editor.long_press(15.0, 20.0, PointerKind::Mouse),
            Some(ContextMenu::Element {
                can_group: false,
                can_ungroup: true
            })
        );

        editor.ungroup();
        assert_eq!(
            editor.long_press(15.0, 20.0, PointerKind::Mouse),
            Some(ContextMenu::Element {
                can_group: true,
                can_ungroup: false
            })
        );
    }

    #[test]
    fn connecting_to_an_http_url_is_refused_before_any_request() {
        let result = connect_webdav("http://example.com".into(), "u".into(), "p".into());

        assert_eq!(result.unwrap_err(), ConnectError::NotHttps);
    }

    #[test]
    fn load_errors_carry_the_cores_message() {
        let error = Editor::new().load(b"[]".to_vec()).unwrap_err();

        assert!(error.to_string().contains("not an Excalidraw Drawing"));
    }

    #[test]
    fn loaded_bytes_are_saved_back() {
        let editor = Editor::new();
        let bytes = Editor::new().save();
        editor.load(bytes.clone()).unwrap();

        assert_eq!(editor.save(), bytes);
    }
}
