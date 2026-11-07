#[derive(Debug, Clone, PartialEq)]
pub enum FilterCriterion {
    TextContains(String),
    ToolFieldEquals {
        tool_name: String,
        field: String,
        value: String,
    },
}

#[derive(Debug, Clone)]
pub struct SessionFilter {
    pub depth: usize,
    pub criteria: Vec<FilterCriterion>,
}
