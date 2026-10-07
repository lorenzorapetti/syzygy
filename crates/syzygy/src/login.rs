//! The login screen: Browser login and Device-code login.
//!
//! `update` is pure: it changes the state and returns the [`Effect`]s for the
//! app to run, so the flow can be tested without TIDAL.

use iced::widget::{button, column, container, qr_code, row, text, text_input};
use iced::{Alignment, Color, Element, Length, task};
use std::sync::Arc;
use std::time::{Duration, Instant};
use syzygy_tidal::auth::{self, PastedInputError, PkceParams};
use syzygy_tidal::models::{AuthTokens, DeviceAuthResponse};
use syzygy_tidal::{LoginMethod, TidalClient};

const MUTED: Color = Color::from_rgb8(0x9C, 0x92, 0xAD);
const ERROR: Color = Color::from_rgb8(0xFF, 0x66, 0x66);

pub struct State {
    screen: Screen,
}

enum Screen {
    Browser(Browser),
    DeviceCode(DeviceCode),
}

struct Browser {
    /// The current attempt. `Err` when this build can't do Browser login.
    pkce: Result<PkceParams, String>,
    /// The sign-in page has been opened, so the paste field shows.
    opened: bool,
    input: String,
    error: Option<BrowserError>,
    /// A code exchange is in flight.
    exchanging: bool,
}

/// Why Browser login is stuck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserError {
    /// TIDAL redirected with `error=`: its description. Only "Start over"
    /// helps.
    Denied(String),
    /// The code exchange failed. Pasting again retries.
    ExchangeFailed(String),
}

enum DeviceCode {
    /// Waiting for TIDAL to hand out a code.
    Requesting,
    /// The code is showing and being polled.
    Showing {
        user_code: String,
        /// `link.tidal.com` with the code filled in.
        link: String,
        qr: Option<Box<qr_code::Data>>,
        /// Aborts the polling `Task` when this state goes away.
        _poll: Option<task::Handle>,
    },
    Expired,
    Failed(String),
}

/// How polling for a Device-code login ended without tokens.
#[derive(Debug, Clone)]
pub enum PollError {
    /// The code's lifetime ran out before the user entered it.
    Expired,
    Failed(Arc<syzygy_tidal::Error>),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// "Sign in with browser", or opening the page again.
    SignInWithBrowser,
    /// Copy the sign-in page's address, for when no browser opened.
    CopyLink,
    InputChanged(String),
    /// Enter in the paste field, or "Sign in".
    Submit,
    PasteFromClipboard,
    /// What "Paste from clipboard" read.
    Pasted(Option<String>),
    Exchanged(Result<AuthTokens, Arc<syzygy_tidal::Error>>),
    /// After TIDAL refused: a new attempt with a new verifier.
    StartOver,
    /// "Use a code instead (no lossless)".
    UseDeviceCode,
    DeviceAuthStarted(Result<DeviceAuthResponse, Arc<syzygy_tidal::Error>>),
    CopyCode,
    OpenDeviceLink,
    DevicePollEnded(Result<AuthTokens, PollError>),
    GetNewCode,
    /// Leave Device-code login for Browser login.
    Cancel,
}

/// Work for the app to do on the login screen's behalf.
#[derive(Debug)]
pub enum Effect {
    OpenUrl(String),
    CopyToClipboard(String),
    /// Read the clipboard into [`Message::Pasted`].
    ReadClipboard,
    /// Finish Browser login.
    ExchangeCode {
        code: String,
        pkce: PkceParams,
    },
    /// Ask TIDAL for a device code, into [`Message::DeviceAuthStarted`].
    StartDeviceAuth,
    /// Run [`poll_device_token`] into [`Message::DevicePollEnded`], and hand
    /// its abort handle to [`State::track_poll`].
    PollDeviceToken {
        device_code: String,
        interval: Duration,
        expires_in: Duration,
    },
    /// Signing in worked: save the Session and open the Shell.
    SignedIn(LoginMethod, AuthTokens),
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            screen: Screen::Browser(Browser::new()),
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match (&mut self.screen, message) {
            (Screen::Browser(browser), Message::SignInWithBrowser) => browser.open(),
            (Screen::Browser(browser), Message::CopyLink) => match &browser.pkce {
                Ok(pkce) => vec![Effect::CopyToClipboard(pkce.authorize_url.clone())],
                Err(_) => Vec::new(),
            },
            (Screen::Browser(browser), Message::InputChanged(input)) => {
                browser.input = input;
                Vec::new()
            }
            (Screen::Browser(browser), Message::Submit) => browser.submit(),
            (Screen::Browser(_), Message::PasteFromClipboard) => vec![Effect::ReadClipboard],
            (Screen::Browser(browser), Message::Pasted(Some(input))) => {
                browser.input = input;
                browser.submit()
            }
            (Screen::Browser(browser), Message::Exchanged(result)) => {
                browser.exchanging = false;
                match result {
                    Ok(tokens) => vec![Effect::SignedIn(LoginMethod::Browser, tokens)],
                    Err(e) => {
                        log::warn!("Browser login code exchange failed: {e}");
                        browser.error = Some(BrowserError::ExchangeFailed(e.to_string()));
                        Vec::new()
                    }
                }
            }
            (Screen::Browser(browser), Message::StartOver) => {
                *browser = Browser::new();
                browser.open()
            }
            (Screen::Browser(browser), Message::UseDeviceCode) if !browser.exchanging => {
                self.request_device_code()
            }
            (
                Screen::DeviceCode(DeviceCode::Expired | DeviceCode::Failed(_)),
                Message::GetNewCode,
            ) => self.request_device_code(),
            (
                Screen::DeviceCode(device @ DeviceCode::Requesting),
                Message::DeviceAuthStarted(result),
            ) => {
                match result {
                    Ok(response) => {
                        let link = device_link(&response);
                        *device = DeviceCode::Showing {
                            user_code: response.user_code,
                            qr: qr_code::Data::new(&link)
                                .inspect_err(|e| log::warn!("No QR code for the device link: {e}"))
                                .ok()
                                .map(Box::new),
                            link,
                            _poll: None,
                        };
                        vec![Effect::PollDeviceToken {
                            device_code: response.device_code,
                            // At least a second, whatever the server says.
                            interval: Duration::from_secs(response.interval.max(1)),
                            expires_in: Duration::from_secs(response.expires_in),
                        }]
                    }
                    Err(e) => {
                        log::warn!("Could not start Device-code login: {e}");
                        *device = DeviceCode::Failed(e.to_string());
                        Vec::new()
                    }
                }
            }
            (Screen::DeviceCode(DeviceCode::Showing { user_code, .. }), Message::CopyCode) => {
                vec![Effect::CopyToClipboard(user_code.clone())]
            }
            (Screen::DeviceCode(DeviceCode::Showing { link, .. }), Message::OpenDeviceLink) => {
                vec![Effect::OpenUrl(link.clone())]
            }
            (
                Screen::DeviceCode(device @ DeviceCode::Showing { .. }),
                Message::DevicePollEnded(result),
            ) => match result {
                Ok(tokens) => vec![Effect::SignedIn(LoginMethod::DeviceCode, tokens)],
                Err(PollError::Expired) => {
                    *device = DeviceCode::Expired;
                    Vec::new()
                }
                Err(PollError::Failed(e)) => {
                    log::warn!("Device-code login failed: {e}");
                    *device = DeviceCode::Failed(e.to_string());
                    Vec::new()
                }
            },
            (Screen::DeviceCode(_), Message::Cancel) => {
                // Dropping the Device-code state aborts its polling.
                self.screen = Screen::Browser(Browser::new());
                Vec::new()
            }
            // Clicks and results meant for a screen that has since gone.
            _ => Vec::new(),
        }
    }

    fn request_device_code(&mut self) -> Vec<Effect> {
        self.screen = Screen::DeviceCode(DeviceCode::Requesting);
        vec![Effect::StartDeviceAuth]
    }

    /// Keep the polling `Task`'s handle, so leaving the code screen aborts it.
    pub fn track_poll(&mut self, handle: task::Handle) {
        if let Screen::DeviceCode(DeviceCode::Showing { _poll, .. }) = &mut self.screen {
            *_poll = Some(handle.abort_on_drop());
        }
    }

    #[cfg(test)]
    /// What's stopping Browser login, if anything.
    pub fn browser_error(&self) -> Option<&BrowserError> {
        match &self.screen {
            Screen::Browser(browser) => browser.error.as_ref(),
            Screen::DeviceCode(_) => None,
        }
    }

    #[cfg(test)]
    /// The device code being shown, if any.
    pub fn device_code(&self) -> Option<&str> {
        match &self.screen {
            Screen::DeviceCode(DeviceCode::Showing { user_code, .. }) => Some(user_code),
            _ => None,
        }
    }

    #[cfg(test)]
    /// Where "Open link.tidal.com" and the QR code go.
    pub fn device_link(&self) -> Option<&str> {
        match &self.screen {
            Screen::DeviceCode(DeviceCode::Showing { link, .. }) => Some(link),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn device_code_expired(&self) -> bool {
        matches!(self.screen, Screen::DeviceCode(DeviceCode::Expired))
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body = match &self.screen {
            Screen::Browser(browser) => browser.view(),
            Screen::DeviceCode(device) => device.view(),
        };
        container(body.max_width(440).spacing(16))
            .center(Length::Fill)
            .padding(24)
            .into()
    }
}

impl Browser {
    fn new() -> Self {
        Self {
            pkce: PkceParams::generate().map_err(|e| e.to_string()),
            opened: false,
            input: String::new(),
            error: None,
            exchanging: false,
        }
    }

    fn open(&mut self) -> Vec<Effect> {
        self.opened = true;
        match &self.pkce {
            Ok(pkce) => vec![Effect::OpenUrl(pkce.authorize_url.clone())],
            Err(_) => Vec::new(),
        }
    }

    fn submit(&mut self) -> Vec<Effect> {
        match auth::parse_pasted_input(&self.input) {
            Err(PastedInputError::Empty) => Vec::new(),
            Err(PastedInputError::Denied(description)) => {
                self.error = Some(BrowserError::Denied(description));
                Vec::new()
            }
            Ok(code) => match &self.pkce {
                Ok(pkce) if !self.exchanging => {
                    self.exchanging = true;
                    self.error = None;
                    vec![Effect::ExchangeCode {
                        code,
                        pkce: pkce.clone(),
                    }]
                }
                _ => Vec::new(),
            },
        }
    }

    fn view(&self) -> iced::widget::Column<'_, Message> {
        let use_code = button(text("Use a code instead (no lossless)").size(13))
            .style(button::text)
            .on_press_maybe((!self.exchanging).then_some(Message::UseDeviceCode));

        if let Err(e) = &self.pkce {
            return column![
                title("Sign in to TIDAL"),
                text(format!("Browser login isn't available in this build: {e}")).color(ERROR),
                use_code,
            ];
        }

        if !self.opened {
            return column![
                title("Sign in to TIDAL"),
                text("Sign in on TIDAL's website, in your own browser.").color(MUTED),
                button(text("Sign in with browser"))
                    .padding([10, 20])
                    .style(button::primary)
                    .on_press(Message::SignInWithBrowser),
                use_code,
            ]
            .align_x(Alignment::Center);
        }

        let field = text_input("Paste the address or the code", &self.input)
            .on_input_maybe((!self.exchanging).then_some(Message::InputChanged))
            .on_submit(Message::Submit)
            .padding(10);
        let paste = button(text("Paste from clipboard"))
            .padding(10)
            .style(button::secondary)
            .on_press_maybe((!self.exchanging).then_some(Message::PasteFromClipboard));
        let sign_in = button(text(if self.exchanging {
            "Signing in…"
        } else {
            "Sign in"
        }))
        .padding([10, 20])
        .style(button::primary)
        .on_press_maybe(
            (!self.exchanging && !self.input.trim().is_empty()).then_some(Message::Submit),
        );

        let error = self.error.as_ref().map(|error| match error {
            BrowserError::Denied(description) => column![
                text(format!("TIDAL said: {description}")).color(ERROR),
                button(text("Start over"))
                    .style(button::secondary)
                    .on_press(Message::StartOver),
            ]
            .spacing(8),
            BrowserError::ExchangeFailed(e) => column![
                text("Signing in didn't work. Paste the address again, or start over.")
                    .color(ERROR),
                text(e.clone()).size(12).color(MUTED),
                button(text("Start over"))
                    .style(button::secondary)
                    .on_press(Message::StartOver),
            ]
            .spacing(8),
        });

        column![
            title("Finish signing in"),
            text(
                "Sign in on the page that opened in your browser. TIDAL then shows a \
                 page that doesn't load: copy its address and paste it here.",
            )
            .color(MUTED),
            row![field, paste].spacing(8),
            sign_in,
        ]
        .push(error)
        .push(
            row![
                button(text("Open the page again").size(13))
                    .style(button::text)
                    .on_press(Message::SignInWithBrowser),
                button(text("Copy link").size(13))
                    .style(button::text)
                    .on_press(Message::CopyLink),
            ]
            .spacing(8),
        )
        .push(use_code)
    }
}

impl DeviceCode {
    fn view(&self) -> iced::widget::Column<'_, Message> {
        let cancel = button(text("Cancel"))
            .style(button::secondary)
            .on_press(Message::Cancel);
        let header = column![
            title("Sign in with a code"),
            text("Device-code login can't play lossless.")
                .size(13)
                .color(MUTED),
        ]
        .spacing(4)
        .align_x(Alignment::Center);

        match self {
            DeviceCode::Requesting => column![header, text("Getting a code…"), cancel],
            DeviceCode::Showing { user_code, qr, .. } => column![
                header,
                text("Go to link.tidal.com on any device and enter:"),
                text(user_code.as_str()).size(40),
                row![
                    button(text("Copy"))
                        .style(button::secondary)
                        .on_press(Message::CopyCode),
                    button(text("Open link.tidal.com"))
                        .style(button::primary)
                        .on_press(Message::OpenDeviceLink),
                ]
                .spacing(8),
            ]
            .push(qr.as_ref().map(|qr| qr_code(qr).cell_size(5)))
            .push(
                text("Waiting for you to enter the code…")
                    .size(13)
                    .color(MUTED),
            )
            .push(cancel),
            DeviceCode::Expired => column![
                header,
                text("This code has expired."),
                button(text("Get a new code"))
                    .style(button::primary)
                    .on_press(Message::GetNewCode),
                cancel,
            ],
            DeviceCode::Failed(e) => column![
                header,
                text("Couldn't sign in with a code.").color(ERROR),
                text(e.as_str()).size(12).color(MUTED),
                button(text("Get a new code"))
                    .style(button::primary)
                    .on_press(Message::GetNewCode),
                cancel,
            ],
        }
        .align_x(Alignment::Center)
    }
}

fn title(label: &str) -> iced::widget::Text<'_> {
    text(label).size(24)
}

/// `verification_uri_complete` (or the bare URI), with a scheme: TIDAL sends
/// `link.tidal.com/CODE`.
fn device_link(response: &DeviceAuthResponse) -> String {
    let uri = response
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&response.verification_uri);
    if uri.contains("://") {
        uri.to_string()
    } else {
        format!("https://{uri}")
    }
}

/// Poll for Device-code tokens every `interval` until the user approves or
/// the code expires. Network failures, rate limits and server errors are
/// retried at the next tick.
pub async fn poll_device_token(
    client: TidalClient,
    device_code: String,
    interval: Duration,
    expires_in: Duration,
) -> Result<AuthTokens, PollError> {
    let deadline = Instant::now() + expires_in;
    loop {
        tokio::time::sleep(interval).await;
        if Instant::now() >= deadline {
            return Err(PollError::Expired);
        }
        match client.poll_device_token(&device_code).await {
            Ok(Some(tokens)) => return Ok(tokens),
            Ok(None) => {}
            Err(e)
                if e.is_network()
                    || matches!(
                        e,
                        syzygy_tidal::Error::Api {
                            status: 429 | 500..,
                            ..
                        }
                    ) =>
            {
                log::warn!("Device-code poll failed, retrying: {e}")
            }
            Err(syzygy_tidal::Error::Api { body, .. }) if body.contains("expired_token") => {
                return Err(PollError::Expired);
            }
            Err(e) => return Err(PollError::Failed(Arc::new(e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened_browser_login() -> State {
        let mut state = State::new();
        state.update(Message::SignInWithBrowser);
        state
    }

    #[test]
    fn a_pasted_error_shows_tidals_description() {
        let mut state = opened_browser_login();

        let effects = state.update(Message::Pasted(Some(
            "https://tidal.com/android/login/auth?error=access_denied&error_description=User%20cancelled"
                .into(),
        )));

        assert!(effects.is_empty());
        assert_eq!(
            state.browser_error(),
            Some(&BrowserError::Denied("User cancelled".into()))
        );
    }

    fn opened_url(effects: &[Effect]) -> &str {
        match effects {
            [Effect::OpenUrl(url)] => url,
            other => panic!("expected one OpenUrl, got {other:?}"),
        }
    }

    #[test]
    fn start_over_opens_a_new_attempt_and_clears_the_error() {
        let mut state = State::new();
        let first = opened_url(&state.update(Message::SignInWithBrowser)).to_string();
        state.update(Message::Pasted(Some("?error=access_denied".into())));

        let effects = state.update(Message::StartOver);

        // A new verifier means a new challenge in the URL.
        assert_ne!(opened_url(&effects), first);
        assert_eq!(state.browser_error(), None);
    }

    fn exchanged_code(effects: &[Effect]) -> &str {
        match effects {
            [Effect::ExchangeCode { code, .. }] => code,
            other => panic!("expected one ExchangeCode, got {other:?}"),
        }
    }

    #[test]
    fn a_pasted_redirect_url_exchanges_its_code() {
        let mut state = opened_browser_login();

        let effects = state.update(Message::Pasted(Some(
            "https://tidal.com/android/login/auth?code=abc123&state=x".into(),
        )));

        assert_eq!(exchanged_code(&effects), "abc123");
    }

    #[test]
    fn a_typed_code_exchanges_on_submit() {
        let mut state = opened_browser_login();
        state.update(Message::InputChanged("  abc123 ".into()));

        let effects = state.update(Message::Submit);

        assert_eq!(exchanged_code(&effects), "abc123");
    }

    fn tokens() -> AuthTokens {
        AuthTokens {
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_in: 3600,
            token_type: "Bearer".into(),
            user_id: Some(42),
        }
    }

    #[test]
    fn a_failed_exchange_shows_inline_and_takes_another_paste() {
        let mut state = opened_browser_login();
        state.update(Message::Pasted(Some("first".into())));

        state.update(Message::Exchanged(Err(Arc::new(
            syzygy_tidal::Error::Api {
                status: 400,
                body: "invalid code".into(),
            },
        ))));

        assert!(matches!(
            state.browser_error(),
            Some(BrowserError::ExchangeFailed(_))
        ));
        let effects = state.update(Message::Pasted(Some("second".into())));
        assert_eq!(exchanged_code(&effects), "second");
    }

    #[test]
    fn a_successful_exchange_signs_in_with_browser_login() {
        let mut state = opened_browser_login();
        state.update(Message::Pasted(Some("abc123".into())));

        let effects = state.update(Message::Exchanged(Ok(tokens())));

        assert!(matches!(
            effects.as_slice(),
            [Effect::SignedIn(LoginMethod::Browser, t)] if *t == tokens()
        ));
    }

    fn device_auth() -> DeviceAuthResponse {
        DeviceAuthResponse {
            device_code: "device".into(),
            user_code: "ABCDE".into(),
            verification_uri: "link.tidal.com".into(),
            verification_uri_complete: Some("link.tidal.com/ABCDE".into()),
            expires_in: 300,
            interval: 2,
        }
    }

    fn waiting_for_device_code() -> State {
        let mut state = State::new();
        state.update(Message::UseDeviceCode);
        state.update(Message::DeviceAuthStarted(Ok(device_auth())));
        state
    }

    #[test]
    fn using_a_code_asks_tidal_for_one() {
        let mut state = State::new();

        let effects = state.update(Message::UseDeviceCode);

        assert!(matches!(effects.as_slice(), [Effect::StartDeviceAuth]));
    }

    #[test]
    fn the_code_shows_and_polls_at_the_servers_interval() {
        let mut state = State::new();
        state.update(Message::UseDeviceCode);

        let effects = state.update(Message::DeviceAuthStarted(Ok(device_auth())));

        assert_eq!(state.device_code(), Some("ABCDE"));
        assert!(matches!(
            effects.as_slice(),
            [Effect::PollDeviceToken { device_code, interval, expires_in }]
                if device_code == "device"
                    && *interval == Duration::from_secs(2)
                    && *expires_in == Duration::from_secs(300)
        ));
    }

    #[test]
    fn the_link_and_qr_code_carry_the_code_over_https() {
        let state = waiting_for_device_code();

        assert_eq!(state.device_link(), Some("https://link.tidal.com/ABCDE"));
    }

    #[test]
    fn an_approved_code_signs_in_with_device_code_login() {
        let mut state = waiting_for_device_code();

        let effects = state.update(Message::DevicePollEnded(Ok(tokens())));

        assert!(matches!(
            effects.as_slice(),
            [Effect::SignedIn(LoginMethod::DeviceCode, t)] if *t == tokens()
        ));
    }

    #[test]
    fn cancel_goes_back_to_browser_login() {
        let mut state = waiting_for_device_code();

        state.update(Message::Cancel);

        assert_eq!(state.device_code(), None);
        assert!(matches!(
            state.update(Message::SignInWithBrowser).as_slice(),
            [Effect::OpenUrl(_)]
        ));
    }

    #[test]
    fn an_expired_code_offers_a_new_one() {
        let mut state = waiting_for_device_code();

        state.update(Message::DevicePollEnded(Err(PollError::Expired)));
        assert!(state.device_code_expired());

        let effects = state.update(Message::GetNewCode);
        assert!(matches!(effects.as_slice(), [Effect::StartDeviceAuth]));
        assert!(!state.device_code_expired());
    }

    #[test]
    fn a_late_poll_result_after_cancel_is_ignored() {
        let mut state = waiting_for_device_code();
        state.update(Message::Cancel);

        let effects = state.update(Message::DevicePollEnded(Ok(tokens())));

        assert!(effects.is_empty());
    }

    /// A successful exchange signs the client in, so its result must land
    /// on the screen that asked for it.
    #[test]
    fn no_switching_to_a_code_while_signing_in() {
        let mut state = opened_browser_login();
        state.update(Message::Pasted(Some("abc123".into())));

        let effects = state.update(Message::UseDeviceCode);
        assert!(effects.is_empty());

        let effects = state.update(Message::Exchanged(Ok(tokens())));
        assert!(matches!(effects.as_slice(), [Effect::SignedIn(..)]));
    }
}
