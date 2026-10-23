//! Внешние сервисы OrbitFM.

mod discovery;
mod radio_browser;

pub(crate) use radio_browser::USER_AGENT;
pub use radio_browser::{RadioBrowserClient, RadioError, SearchParams, Station};
