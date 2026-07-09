pub(crate) struct Setting {
    display_name: String,
    data: SettingData,
}

pub(crate) enum SettingData {
    Boolean(bool),
    Number(u16),
    String(String),
}

pub(crate) trait SettingCollection {}
