use yaml_rust::parser::{Event, MarkedEventReceiver, Parser};
use yaml_rust::scanner::{Marker, Scanner, TokenType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Node {
    Scalar {
        value: String,
        style: String,
        tag: String,
    },
    Mapping(Vec<Node>),
    Sequence(Vec<Node>),
}

impl Node {
    pub(super) fn name(&self) -> Result<&str, String> {
        match self {
            Self::Scalar { value, .. } => Ok(value),
            _ => Err("YAML mapping key must be scalar".into()),
        }
    }
}

#[derive(Default)]
pub(super) struct Syntax {
    stack: Vec<Node>,
    root: Option<Node>,
    documents: usize,
    error: Option<String>,
}

impl Syntax {
    pub(super) fn parse(source: &str) -> Result<Node, String> {
        if source.len() > 1024 * 1024 {
            return Err("CI workflow exceeds 1 MiB".into());
        }
        for token in Scanner::new(source.chars()) {
            if matches!(
                token.1,
                TokenType::Tag(_, _)
                    | TokenType::TagDirective(_, _)
                    | TokenType::VersionDirective(_, _)
            ) {
                return Err("Tagged or versioned YAML requires full validation".into());
            }
        }
        let mut syntax = Self::default();
        Parser::new(source.chars())
            .load(&mut syntax, true)
            .map_err(|error| error.to_string())?;
        if let Some(error) = syntax.error {
            return Err(error);
        }
        if syntax.documents != 1 || !syntax.stack.is_empty() {
            return Err("One complete YAML document is required".into());
        }
        let Some(root @ Node::Mapping(_)) = syntax.root else {
            return Err("Workflow must be a YAML mapping".into());
        };
        Ok(root)
    }

    fn add(&mut self, node: Node) {
        match self.stack.last_mut() {
            Some(Node::Mapping(children) | Node::Sequence(children)) => children.push(node),
            Some(Node::Scalar { .. }) => self.error = Some("Invalid YAML parent".into()),
            None if self.root.is_none() => self.root = Some(node),
            None => self.error = Some("Multiple YAML roots".into()),
        }
    }
}

impl MarkedEventReceiver for Syntax {
    fn on_event(&mut self, event: Event, _: Marker) {
        if self.error.is_some() {
            return;
        }
        match event {
            Event::DocumentStart => self.documents += 1,
            Event::MappingStart(0) => self.stack.push(Node::Mapping(Vec::new())),
            Event::SequenceStart(0) => self.stack.push(Node::Sequence(Vec::new())),
            Event::MappingEnd | Event::SequenceEnd => match self.stack.pop() {
                Some(node) => self.add(node),
                None => self.error = Some("Unbalanced YAML collection".into()),
            },
            Event::Scalar(value, style, 0, tag) => self.add(Node::Scalar {
                value,
                style: format!("{style:?}"),
                tag: format!("{tag:?}"),
            }),
            Event::StreamStart | Event::StreamEnd | Event::DocumentEnd | Event::Nothing => {}
            _ => self.error = Some("YAML anchors and aliases are not presentation-only".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ci::plan::Plan;

    #[test]
    fn orchestration_names_preserve_execution_and_all_other_inputs_are_checked() {
        let source = "name: CI\non: pull_request\njobs:\n  ci:\n    name: CI\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n";
        assert!(Plan::equivalent(
            source,
            &source.replace("    name: CI", "    name: Results")
        ));
        for changed in [
            source.replace("run: true", "run: yes"),
            source.replace("ubuntu-latest", "macos-latest"),
            source.replace("name: CI\non:", "name: Other\non:"),
        ] {
            assert!(!Plan::equivalent(source, &changed));
        }
    }

    #[test]
    fn duplicate_keys_aliases_and_scalar_styles_never_hide_executable_changes() {
        let source = "jobs:\n  ci:\n    name: Results\n    steps:\n      - run: yes\n";
        for changed in [
            "jobs: [",
            "jobs:\n  ci:\n    name: Results\n    name: Other\n",
            "jobs:\n  ci: &gate\n    name: Results\n  other: *gate\n",
            "---\n---\n",
            "jobs: !!map {ci: {name: Results}}",
        ] {
            assert!(!Plan::equivalent(source, changed));
        }
        assert!(!Plan::equivalent(
            source,
            &source.replace("run: yes", "run: 'yes'")
        ));
    }
}
