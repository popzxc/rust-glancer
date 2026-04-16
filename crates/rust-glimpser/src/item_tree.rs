use anyhow::Context as _;
use ra_syntax::{
    Edition, SourceFile, TextRange,
    ast::{self, AstNode, HasModuleItem, HasName, HasVisibility},
};
use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateItemTree {
    pub entry_file: PathBuf,
    pub root_items: Vec<ItemNode>,
    pub parse_errors: Vec<ParseError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub file_path: PathBuf,
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemNode {
    pub kind: ItemKind,
    pub name: Option<String>,
    pub visibility: VisibilityLevel,
    pub file_path: PathBuf,
    pub span: Span,
    pub children: Vec<ItemNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    AsmExpr,
    AssociatedConst,
    AssociatedFunction,
    AssociatedTypeAlias,
    Const,
    Enum,
    ExternBlock,
    ExternCrate,
    Function,
    Impl,
    MacroDefinition,
    Module,
    Static,
    Struct,
    Trait,
    TypeAlias,
    Union,
    Use,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisibilityLevel {
    Private,
    Public,
    Crate,
    Super,
    Self_,
    Restricted(String),
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub text: TextSpan,
    pub line_column: LineColumnSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineColumnSpan {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

pub fn build_crate_item_tree(entry_file: PathBuf) -> anyhow::Result<CrateItemTree> {
    let entry_file = entry_file
        .canonicalize()
        .with_context(|| format!("while attempting to canonicalize {}", entry_file.display()))?;

    let mut collector = ItemCollector::default();
    let root_items = collector.collect_file_items(&entry_file).with_context(|| {
        format!(
            "while attempting to collect items from {}",
            entry_file.display()
        )
    })?;

    Ok(CrateItemTree {
        entry_file,
        root_items,
        parse_errors: collector.parse_errors,
    })
}

pub fn print_tree(tree: &CrateItemTree) {
    println!("Item tree for {}", tree.entry_file.display());
    for item in &tree.root_items {
        print_item(item, 0);
    }

    if !tree.parse_errors.is_empty() {
        println!();
        println!("Parser errors:");
        for parse_error in &tree.parse_errors {
            println!(
                "- {}:{}:{} [{}..{}]: {}",
                parse_error.file_path.display(),
                parse_error.span.line_column.start.line + 1,
                parse_error.span.line_column.start.column + 1,
                parse_error.span.text.start,
                parse_error.span.text.end,
                parse_error.message,
            );
        }
    }
}

fn print_item(item: &ItemNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let name = item.name.as_deref().unwrap_or("<anonymous>");
    println!(
        "{indent}- {} {name} [{}] {}:{}:{}-{}:{} ({}..{})",
        item.kind,
        item.visibility,
        item.file_path.display(),
        item.span.line_column.start.line + 1,
        item.span.line_column.start.column + 1,
        item.span.line_column.end.line + 1,
        item.span.line_column.end.column + 1,
        item.span.text.start,
        item.span.text.end,
    );

    for child in &item.children {
        print_item(child, depth + 1);
    }
}

#[derive(Default)]
struct ItemCollector {
    visited_files: HashSet<PathBuf>,
    parse_errors: Vec<ParseError>,
}

impl ItemCollector {
    fn collect_file_items(&mut self, file_path: &Path) -> anyhow::Result<Vec<ItemNode>> {
        let file_path = file_path
            .canonicalize()
            .with_context(|| format!("while attempting to canonicalize {}", file_path.display()))?;

        if !self.visited_files.insert(file_path.to_path_buf()) {
            return Ok(Vec::new());
        }

        let source = std::fs::read_to_string(&file_path)
            .with_context(|| format!("while attempting to read {}", file_path.display()))?;
        let line_index = LineIndex::new(&source);
        let parsed_file = SourceFile::parse(&source, Edition::CURRENT);

        for error in parsed_file.errors() {
            self.parse_errors.push(ParseError {
                file_path: file_path.to_path_buf(),
                message: error.to_string(),
                span: span_from_range(error.range(), &line_index),
            });
        }

        self.collect_items(parsed_file.tree().items(), &file_path, &line_index)
            .with_context(|| {
                format!(
                    "while attempting to collect items from {}",
                    file_path.display()
                )
            })
    }

    fn collect_items(
        &mut self,
        items: impl Iterator<Item = ast::Item>,
        current_file_path: &Path,
        line_index: &LineIndex,
    ) -> anyhow::Result<Vec<ItemNode>> {
        let mut nodes = Vec::new();

        for item in items {
            let node = match item {
                ast::Item::AsmExpr(item) => Some(build_item_node(
                    ItemKind::AsmExpr,
                    None,
                    VisibilityLevel::Private,
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Const(item) => Some(build_item_node(
                    ItemKind::Const,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Enum(item) => Some(build_item_node(
                    ItemKind::Enum,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::ExternBlock(item) => Some(build_item_node(
                    ItemKind::ExternBlock,
                    None,
                    VisibilityLevel::Private,
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::ExternCrate(item) => Some(build_item_node(
                    ItemKind::ExternCrate,
                    item.name_ref()
                        .map(|name_ref| name_ref.syntax().text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Fn(item) => Some(build_item_node(
                    ItemKind::Function,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Impl(item) => {
                    let children = self.collect_impl_items(&item, current_file_path, line_index);
                    Some(build_item_node(
                        ItemKind::Impl,
                        None,
                        visibility_level(item.visibility()),
                        item.syntax().text_range(),
                        current_file_path,
                        line_index,
                        children,
                    ))
                }
                ast::Item::MacroCall(_) => None,
                ast::Item::MacroDef(item) => Some(build_item_node(
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::MacroRules(item) => Some(build_item_node(
                    ItemKind::MacroDefinition,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Module(item) => {
                    let module_name = item.name().map(|name| name.text().to_string());
                    let children = self
                        .collect_module_children(&item, current_file_path, line_index)
                        .with_context(|| {
                            format!(
                                "while attempting to collect module children for {} in {}",
                                module_name.as_deref().unwrap_or("<unnamed>"),
                                current_file_path.display()
                            )
                        })?;
                    Some(build_item_node(
                        ItemKind::Module,
                        module_name,
                        visibility_level(item.visibility()),
                        item.syntax().text_range(),
                        current_file_path,
                        line_index,
                        children,
                    ))
                }
                ast::Item::Static(item) => Some(build_item_node(
                    ItemKind::Static,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Struct(item) => Some(build_item_node(
                    ItemKind::Struct,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Trait(item) => Some(build_item_node(
                    ItemKind::Trait,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::TypeAlias(item) => Some(build_item_node(
                    ItemKind::TypeAlias,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Union(item) => Some(build_item_node(
                    ItemKind::Union,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::Item::Use(item) => Some(build_item_node(
                    ItemKind::Use,
                    None,
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
            };

            if let Some(node) = node {
                nodes.push(node);
            }
        }

        Ok(nodes)
    }

    fn collect_impl_items(
        &self,
        item: &ast::Impl,
        current_file_path: &Path,
        line_index: &LineIndex,
    ) -> Vec<ItemNode> {
        let Some(assoc_item_list) = item.assoc_item_list() else {
            return Vec::new();
        };

        let mut children = Vec::new();
        for assoc_item in assoc_item_list.assoc_items() {
            let node = match assoc_item {
                ast::AssocItem::Const(item) => Some(build_item_node(
                    ItemKind::AssociatedConst,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::AssocItem::Fn(item) => Some(build_item_node(
                    ItemKind::AssociatedFunction,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::AssocItem::TypeAlias(item) => Some(build_item_node(
                    ItemKind::AssociatedTypeAlias,
                    item.name().map(|name| name.text().to_string()),
                    visibility_level(item.visibility()),
                    item.syntax().text_range(),
                    current_file_path,
                    line_index,
                    Vec::new(),
                )),
                ast::AssocItem::MacroCall(_) => None,
            };

            if let Some(node) = node {
                children.push(node);
            }
        }

        children
    }

    fn collect_module_children(
        &mut self,
        item: &ast::Module,
        current_file_path: &Path,
        line_index: &LineIndex,
    ) -> anyhow::Result<Vec<ItemNode>> {
        if let Some(item_list) = item.item_list() {
            return self
                .collect_items(item_list.items(), current_file_path, line_index)
                .with_context(|| {
                    format!(
                        "while attempting to collect inline module items in {}",
                        current_file_path.display()
                    )
                });
        }

        let Some(module_name) = item.name().map(|name| name.text().to_string()) else {
            return Ok(Vec::new());
        };

        // TODO: support `#[path = "..."]` and other advanced module-resolution rules when needed.
        let Some(module_file_path) = resolve_module_file(current_file_path, &module_name) else {
            return Ok(Vec::new());
        };

        self.collect_file_items(&module_file_path).with_context(|| {
            format!(
                "while attempting to collect file module items from {}",
                module_file_path.display()
            )
        })
    }
}

fn build_item_node(
    kind: ItemKind,
    name: Option<String>,
    visibility: VisibilityLevel,
    text_range: TextRange,
    current_file_path: &Path,
    line_index: &LineIndex,
    children: Vec<ItemNode>,
) -> ItemNode {
    ItemNode {
        kind,
        name,
        visibility,
        file_path: current_file_path.to_path_buf(),
        span: span_from_range(text_range, line_index),
        children,
    }
}

fn visibility_level(visibility: Option<ast::Visibility>) -> VisibilityLevel {
    let Some(visibility) = visibility else {
        return VisibilityLevel::Private;
    };

    let Some(inner) = visibility.visibility_inner() else {
        return VisibilityLevel::Public;
    };

    let Some(path) = inner.path() else {
        return VisibilityLevel::Unknown(visibility.syntax().text().to_string());
    };
    let path_text = path.syntax().text().to_string();

    if inner.in_token().is_some() {
        return VisibilityLevel::Restricted(path_text);
    }

    match path_text.as_str() {
        "crate" => VisibilityLevel::Crate,
        "super" => VisibilityLevel::Super,
        "self" => VisibilityLevel::Self_,
        _ => VisibilityLevel::Unknown(visibility.syntax().text().to_string()),
    }
}

fn resolve_module_file(current_file_path: &Path, module_name: &str) -> Option<PathBuf> {
    let parent_dir = current_file_path.parent()?;
    let file_name = current_file_path.file_name()?.to_str()?;
    let file_stem = current_file_path.file_stem()?.to_str()?;

    let module_parent = match file_name {
        "lib.rs" | "main.rs" | "mod.rs" => parent_dir.to_path_buf(),
        _ => parent_dir.join(file_stem),
    };

    let flat_file = module_parent.join(format!("{module_name}.rs"));
    if flat_file.exists() {
        return Some(flat_file);
    }

    let nested_file = module_parent.join(module_name).join("mod.rs");
    if nested_file.exists() {
        return Some(nested_file);
    }

    None
}

fn span_from_range(text_range: TextRange, line_index: &LineIndex) -> Span {
    let start = u32::from(text_range.start());
    let end = u32::from(text_range.end());

    Span {
        text: TextSpan { start, end },
        line_column: LineColumnSpan {
            start: line_index.position(start),
            end: line_index.position(end),
        },
    }
}

#[derive(Debug)]
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, byte) in source.as_bytes().iter().enumerate() {
            if *byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self { line_starts }
    }

    fn position(&self, offset: u32) -> Position {
        let offset = usize::try_from(offset).expect("offset should fit into usize");
        let line_index = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };
        let line_start = self.line_starts[line_index];
        let column = offset.saturating_sub(line_start);

        Position {
            line: u32::try_from(line_index).expect("line index should fit into u32"),
            column: u32::try_from(column).expect("column should fit into u32"),
        }
    }
}

impl fmt::Display for ItemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            ItemKind::AsmExpr => "asm",
            ItemKind::AssociatedConst => "associated_const",
            ItemKind::AssociatedFunction => "associated_fn",
            ItemKind::AssociatedTypeAlias => "associated_type_alias",
            ItemKind::Const => "const",
            ItemKind::Enum => "enum",
            ItemKind::ExternBlock => "extern_block",
            ItemKind::ExternCrate => "extern_crate",
            ItemKind::Function => "fn",
            ItemKind::Impl => "impl",
            ItemKind::MacroDefinition => "macro_definition",
            ItemKind::Module => "module",
            ItemKind::Static => "static",
            ItemKind::Struct => "struct",
            ItemKind::Trait => "trait",
            ItemKind::TypeAlias => "type_alias",
            ItemKind::Union => "union",
            ItemKind::Use => "use",
        };
        write!(f, "{value}")
    }
}

impl fmt::Display for VisibilityLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VisibilityLevel::Private => write!(f, "private"),
            VisibilityLevel::Public => write!(f, "pub"),
            VisibilityLevel::Crate => write!(f, "pub(crate)"),
            VisibilityLevel::Super => write!(f, "pub(super)"),
            VisibilityLevel::Self_ => write!(f, "pub(self)"),
            VisibilityLevel::Restricted(path) => write!(f, "pub(in {path})"),
            VisibilityLevel::Unknown(raw) => write!(f, "{raw}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ItemKind, VisibilityLevel, build_crate_item_tree};
    use std::path::PathBuf;

    fn test_file(path: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test_targets")
            .join(path)
    }

    fn flatten<'a>(items: &'a [super::ItemNode], output: &mut Vec<&'a super::ItemNode>) {
        for item in items {
            output.push(item);
            flatten(&item.children, output);
        }
    }

    #[test]
    fn parses_module_tree_and_impl_items() {
        let tree = build_crate_item_tree(test_file("moderate_crate/src/lib.rs"))
            .expect("fixture crate should parse");
        let mut all_items = Vec::new();
        flatten(&tree.root_items, &mut all_items);

        let model_module = all_items
            .iter()
            .find(|item| item.kind == ItemKind::Module && item.name.as_deref() == Some("model"))
            .expect("model module should exist");
        assert_eq!(model_module.visibility, VisibilityLevel::Public);

        let constructor = all_items
            .iter()
            .find(|item| {
                item.kind == ItemKind::AssociatedFunction && item.name.as_deref() == Some("new")
            })
            .expect("impl method should be collected");
        assert_eq!(constructor.visibility, VisibilityLevel::Public);
    }

    #[test]
    fn keeps_macro_definitions_only() {
        let tree = build_crate_item_tree(test_file("complex_crate/src/lib.rs"))
            .expect("fixture crate should parse");
        let mut all_items = Vec::new();
        flatten(&tree.root_items, &mut all_items);

        let macro_def = all_items
            .iter()
            .find(|item| {
                item.kind == ItemKind::MacroDefinition
                    && item.name.as_deref() == Some("label_result")
            })
            .expect("macro definition should exist");
        assert_eq!(macro_def.visibility, VisibilityLevel::Private);
    }

    #[test]
    fn stores_offset_and_line_column_spans() {
        let tree = build_crate_item_tree(test_file("simple_crate/src/lib.rs"))
            .expect("fixture crate should parse");
        let mut all_items = Vec::new();
        flatten(&tree.root_items, &mut all_items);

        let function = all_items
            .iter()
            .find(|item| {
                item.kind == ItemKind::Function && item.name.as_deref() == Some("add_two_numbers")
            })
            .expect("function should exist");
        assert!(function.span.text.end > function.span.text.start);
        assert!(
            function.span.line_column.end.line >= function.span.line_column.start.line,
            "span end line should be after start line"
        );
    }
}
