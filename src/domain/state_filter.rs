/// A generic list-state selector shared by issue and pull-request views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Closed,
    All,
}
