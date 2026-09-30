use thiserror::Error;

/// Errors produced by the core tree and its registries.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("node id must not be empty")]
    EmptyNodeId,

    #[error("node `{0}` already exists in the tree")]
    DuplicateNode(String),

    #[error("node `{id}` does not exist")]
    UnknownNode { id: String },

    #[error("node `{child}` references parent `{parent}`, which does not exist")]
    MissingParent { child: String, parent: String },

    #[error("the root node cannot be removed")]
    RemoveRoot,

    #[error("node type `{0}` is not registered")]
    UnknownNodeType(String),

    #[error("node `{0}` has an empty name")]
    EmptyNodeName(String),
}

/// Convenience alias for core results.
pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_render_readable_messages() {
        assert_eq!(
            CoreError::DuplicateNode("root".into()).to_string(),
            "node `root` already exists in the tree"
        );
    }
}