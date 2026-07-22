/// A named configuration setting with a typed value.
pub(crate) struct Setting {
    display_name: String,
    data: SettingData,
}

/// The possible value types for a [`Setting`].
pub(crate) enum SettingData {
    Boolean(bool),
    Number(u16),
    String(String),
}

/// Marker trait for a collection of related settings (e.g. all test-mode settings).
pub(crate) trait SettingCollection {}
