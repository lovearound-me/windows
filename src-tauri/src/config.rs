/// needed-server on Cloud Run (REST + WS /v1/stream).
pub const API_BASE: &str = "https://api.lovearound.me";
/// Firebase Web API key: public, identifies the project to Firebase Auth.
pub const FIREBASE_API_KEY: &str = "AIzaSyBZJfln_KXsxRxo3fvVaV1ePmiVUIn5YXY";
/// The site: install page, invite links /i/<code>, Google sign-in bridge /signin/.
pub const SITE: &str = "https://lovearound.me";
/// Links the app opens: lovearound://add/<code>, lovearound://signin?….
pub const URL_SCHEME: &str = "lovearound";
/// Plus the codename's needed://, which older builds of the site still send.
pub const URL_SCHEMES: [&str; 2] = [URL_SCHEME, "needed"];
