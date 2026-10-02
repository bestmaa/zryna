//! Internal abstract states; none is deserializable or an executable authority.

use super::ValueType;
use zryna_syntax::native_c_v0::raw::Category;

#[derive(Clone, Debug)]
pub(super) struct Value {
    pub ty: ValueType,
    pub token: Option<usize>,
    pub status: Option<usize>,
    pub binding: Option<usize>,
    pub length_of: Option<usize>,
}
impl Value {
    pub fn plain(ty: ValueType) -> Self {
        Self { ty, token: None, status: None, binding: None, length_of: None }
    }
    pub fn token(ty: ValueType, token: usize) -> Self {
        Self { token: Some(token), ..Self::plain(ty) }
    }
}
#[derive(Clone, Debug)]
pub(super) struct Binding {
    pub value: Value,
    pub available: bool,
}
#[derive(Clone, Debug)]
pub(super) struct Token {
    pub ty: ValueType,
    pub live: bool,
    pub kind: Option<String>,
    pub output: Option<Output>,
    pub owner: Option<usize>,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct Output {
    pub call: usize,
    pub group: Option<usize>,
    pub owner: Option<usize>,
}
#[derive(Clone, Debug)]
pub(super) struct Call {
    pub operation: usize,
    pub proved_zero: bool,
    pub status_mode: bool,
}
#[derive(Clone, Debug)]
pub(super) struct Owner {
    pub call: usize,
    pub group: usize,
    pub primary_slot: usize,
    pub slots: Vec<usize>,
    pub token: Option<usize>,
    pub kind: String,
    pub library: String,
    pub allocator: String,
    pub release: String,
    pub category: Category,
    pub releasable_on_malformed: bool,
    pub live: bool,
    pub validated: bool,
}
