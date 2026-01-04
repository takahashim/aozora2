//! ブロック管理
//!
//! ブロック要素のスタック管理を行います。

use aozora_core::node::{BlockParams, BlockType, MidashiLevel};

use super::tag_generator::{generate_block_end_tag, generate_block_start_tag};

/// ブロックコンテキスト
#[derive(Debug, Clone)]
pub struct BlockContext {
    pub block_type: BlockType,
    pub params: BlockParams,
}

/// ブロックマネージャー
#[derive(Debug, Clone, Default)]
pub struct BlockManager {
    /// 現在のブロックスタック
    stack: Vec<BlockContext>,
    /// 見出しIDカウンター
    midashi_id_counter: u32,
}

impl BlockManager {
    /// 新しいブロックマネージャーを作成
    pub fn new() -> Self {
        Self::default()
    }

    /// スタックの長さを取得
    pub fn stack_len(&self) -> usize {
        self.stack.len()
    }

    /// ブロックをプッシュ
    pub fn push(&mut self, block_type: BlockType, params: BlockParams) {
        self.stack.push(BlockContext { block_type, params });
    }

    /// ブロックをポップ
    pub fn pop(&mut self) -> Option<BlockContext> {
        self.stack.pop()
    }

    /// スタックの指定位置からブロックを削除
    #[allow(dead_code)]
    pub fn remove(&mut self, pos: usize) -> BlockContext {
        self.stack.remove(pos)
    }

    /// スタックを走査してコンテキストを参照
    #[allow(dead_code)]
    pub fn iter(&self) -> impl Iterator<Item = &BlockContext> {
        self.stack.iter()
    }

    /// スタックが空でないかどうか
    #[allow(dead_code)]
    pub fn has_blocks(&self) -> bool {
        !self.stack.is_empty()
    }

    /// 指定された長さまでスタックをポップし、終了タグを生成
    pub fn pop_to_length(&mut self, target_len: usize) -> Vec<(BlockType, BlockParams)> {
        let mut result = Vec::new();
        while self.stack.len() > target_len {
            if let Some(ctx) = self.stack.pop() {
                result.push((ctx.block_type, ctx.params));
            }
        }
        result
    }

    /// ぶら下げブロック内かどうかをチェックし、パラメータを返す
    pub fn find_burasage_context(&self) -> Option<(u32, i32)> {
        for ctx in &self.stack {
            if ctx.block_type == BlockType::Burasage {
                let wrap_width = ctx.params.wrap_width.unwrap_or(1);
                let width = ctx.params.width.unwrap_or(0);
                let text_indent = width as i32 - wrap_width as i32;
                return Some((wrap_width, text_indent));
            }
        }
        None
    }

    /// インラインブロック（is_block = false）を探して削除
    pub fn close_inline_blocks(&mut self) -> Vec<(BlockType, BlockParams)> {
        let mut result = Vec::new();
        while let Some(pos) = self.stack.iter().rposition(|c| !c.params.is_block) {
            let ctx = self.stack.remove(pos);
            result.push((ctx.block_type, ctx.params));
        }
        result
    }

    /// 新しいブロック開始時に関連ブロックを閉じる
    pub fn close_related_blocks(
        &mut self,
        new_block_type: &BlockType,
    ) -> Vec<(BlockType, BlockParams)> {
        let mut result = Vec::new();

        // レイアウトブロック以外は何も閉じない
        if !new_block_type.is_layout_block() {
            return result;
        }

        // 閉じるべきブロックを検索して削除
        while let Some(pos) = self
            .stack
            .iter()
            .rposition(|c| c.block_type.should_close_for(new_block_type))
        {
            let ctx = self.stack.remove(pos);
            // Burasageは終了タグを出力しない
            if ctx.block_type != BlockType::Burasage {
                result.push((ctx.block_type, ctx.params));
            }
        }

        result
    }

    /// 対応するブロック終了を探して削除
    pub fn find_and_close(&mut self, block_type: &BlockType) -> Option<BlockContext> {
        // block_type.closes_block() で閉じられるべきブロックを検索
        let pos = self
            .stack
            .iter()
            .rposition(|c| block_type.closes_block(&c.block_type));

        pos.map(|p| self.stack.remove(p))
    }

    /// 見出しIDを生成
    pub fn generate_midashi_id(&mut self, level: MidashiLevel) -> u32 {
        let increment = match level {
            MidashiLevel::O => 100,
            MidashiLevel::Naka => 10,
            MidashiLevel::Ko => 1,
        };
        self.midashi_id_counter += increment;
        self.midashi_id_counter
    }

    /// ブロック開始タグを生成
    pub fn render_block_start_tag(
        &mut self,
        block_type: &BlockType,
        params: &BlockParams,
    ) -> String {
        // 見出しの場合はIDを生成
        let midashi_id = if *block_type == BlockType::Midashi {
            let level = params.level.unwrap_or(MidashiLevel::O);
            Some(self.generate_midashi_id(level))
        } else {
            None
        };

        generate_block_start_tag(block_type, params, midashi_id)
    }

    /// ブロック終了タグを生成
    pub fn render_block_end_tag(&self, block_type: &BlockType, params: &BlockParams) -> String {
        generate_block_end_tag(block_type, params)
    }
}
