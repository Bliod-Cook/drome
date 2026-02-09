use std::collections::BTreeMap;

use core_types::UiLanguage;

#[derive(Debug, Clone)]
pub struct I18n {
    lang: UiLanguage,
    zh_cn: BTreeMap<&'static str, &'static str>,
    en_us: BTreeMap<&'static str, &'static str>,
}

impl I18n {
    pub fn new(lang: UiLanguage) -> Self {
        Self {
            lang,
            zh_cn: zh_cn_map(),
            en_us: en_us_map(),
        }
    }

    pub fn set_language(&mut self, lang: UiLanguage) {
        self.lang = lang;
    }

    pub fn language(&self) -> UiLanguage {
        self.lang
    }

    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        match self.lang {
            UiLanguage::ZhCn => self
                .zh_cn
                .get(key)
                .copied()
                .or_else(|| self.en_us.get(key).copied())
                .unwrap_or(key),
            UiLanguage::EnUs => self
                .en_us
                .get(key)
                .copied()
                .or_else(|| self.zh_cn.get(key).copied())
                .unwrap_or(key),
        }
    }
}

fn zh_cn_map() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("app.title", "Drome AI 客户端"),
        ("nav.chat", "会话"),
        ("nav.settings", "配置"),
        ("chat.placeholder", "M1 文本聊天入口（多模态将在 M2 支持）"),
        ("chat.send", "发送"),
        ("settings.providers", "模型服务"),
        ("settings.mcp", "MCP 服务器"),
        ("settings.language", "界面语言"),
        ("settings.encryption", "本地加密"),
        ("settings.encryption.off", "默认关闭"),
        ("settings.encryption.on", "已开启"),
    ])
}

fn en_us_map() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("app.title", "Drome AI Client"),
        ("nav.chat", "Chat"),
        ("nav.settings", "Settings"),
        (
            "chat.placeholder",
            "M1 text chat entrypoint (multimodal in M2)",
        ),
        ("chat.send", "Send"),
        ("settings.providers", "Providers"),
        ("settings.mcp", "MCP Servers"),
        ("settings.language", "Language"),
        ("settings.encryption", "Local Encryption"),
        ("settings.encryption.off", "Off by default"),
        ("settings.encryption.on", "Enabled"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_chinese_translation() {
        let i18n = I18n::new(UiLanguage::ZhCn);
        assert_eq!(i18n.t("nav.chat"), "会话");
    }

    #[test]
    fn falls_back_to_key_when_missing() {
        let i18n = I18n::new(UiLanguage::EnUs);
        assert_eq!(i18n.t("not.exists"), "not.exists");
    }
}
