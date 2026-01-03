//! 見出し関連の型定義

/// 見出しレベル
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidashiLevel {
    /// 大見出し → h3
    O,
    /// 中見出し → h4
    Naka,
    /// 小見出し → h5
    Ko,
}

impl MidashiLevel {
    /// コマンド名から見出しレベルを取得
    pub fn from_command(command: &str) -> Option<Self> {
        if command.contains("大見出し") {
            Some(MidashiLevel::O)
        } else if command.contains("中見出し") {
            Some(MidashiLevel::Naka)
        } else if command.contains("小見出し") {
            Some(MidashiLevel::Ko)
        } else {
            None
        }
    }
}

/// 見出しスタイル
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MidashiStyle {
    /// 通常（独立行）
    #[default]
    Normal,
    /// 同行（本文と同じ行）
    Dogyo,
    /// 窓見出し
    Mado,
}

impl MidashiStyle {
    /// コマンド名から見出しスタイルを取得
    pub fn from_command(command: &str) -> Self {
        if command.contains("同行") {
            MidashiStyle::Dogyo
        } else if command.contains("窓") {
            MidashiStyle::Mado
        } else {
            MidashiStyle::Normal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midashi_level_from_command() {
        assert_eq!(
            MidashiLevel::from_command("大見出し"),
            Some(MidashiLevel::O)
        );
        assert_eq!(
            MidashiLevel::from_command("中見出し"),
            Some(MidashiLevel::Naka)
        );
        assert_eq!(
            MidashiLevel::from_command("小見出し"),
            Some(MidashiLevel::Ko)
        );
    }

    #[test]
    fn test_midashi_style_from_command() {
        assert_eq!(MidashiStyle::from_command("大見出し"), MidashiStyle::Normal);
        assert_eq!(
            MidashiStyle::from_command("同行大見出し"),
            MidashiStyle::Dogyo
        );
        assert_eq!(MidashiStyle::from_command("窓大見出し"), MidashiStyle::Mado);
    }
}
