use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
};

use proc_macro2::Span;
use syn::{
    ImplItem, Item, ItemExternCrate, ItemImpl, ItemMod, ItemUse, Path as SynPath, Type, UseTree,
    spanned::Spanned, visit::Visit,
};
use walkdir::WalkDir;

type ModulePath = Vec<String>;
type RootAliases = HashSet<(ModulePath, String)>;

#[derive(Debug, PartialEq, Eq)]
struct Violation {
    file: String,
    line: usize,
    target: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}:{} depends on forbidden target {}",
            self.file, self.line, self.target
        )
    }
}

struct DependencyVisitor<'a> {
    file: &'a Path,
    module_path: ModulePath,
    root_aliases: &'a RootAliases,
    forbidden: fn(&[String]) -> bool,
    violations: Vec<Violation>,
}

impl DependencyVisitor<'_> {
    fn record(&mut self, span: Span, segments: &[String]) {
        let target = resolve_path(segments, &self.module_path, self.root_aliases);
        if (self.forbidden)(&target) {
            self.violations.push(Violation {
                file: self.file.display().to_string(),
                line: span.start().line,
                target: target.join("::"),
            });
        }
    }
}

impl<'ast> Visit<'ast> for DependencyVisitor<'_> {
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        let mut targets = Vec::new();
        collect_use_targets(&item.tree, &mut Vec::new(), &mut targets);
        for target in targets {
            self.record(item.use_token.span, &target);
        }
    }

    fn visit_path(&mut self, path: &'ast SynPath) {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        self.record(
            path.segments
                .first()
                .map_or(path.span(), |segment| segment.ident.span()),
            &segments,
        );
        syn::visit::visit_path(self, path);
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if item.content.is_some() {
            self.module_path.push(item.ident.to_string());
            syn::visit::visit_item_mod(self, item);
            self.module_path.pop();
        }
    }
}

#[derive(Debug)]
struct RootAliasCandidate {
    module_path: ModulePath,
    alias: String,
    source: Vec<String>,
}

struct RootAliasCollector {
    module_path: ModulePath,
    candidates: Vec<RootAliasCandidate>,
}

impl<'ast> Visit<'ast> for RootAliasCollector {
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        collect_root_alias_candidates(
            &item.tree,
            &mut Vec::new(),
            &self.module_path,
            &mut self.candidates,
        );
    }

    fn visit_item_extern_crate(&mut self, item: &'ast ItemExternCrate) {
        if item.ident == "self"
            && let Some((_, rename)) = &item.rename
        {
            self.candidates.push(RootAliasCandidate {
                module_path: self.module_path.clone(),
                alias: rename.to_string(),
                source: vec!["crate".to_string()],
            });
        }
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if item.content.is_some() {
            self.module_path.push(item.ident.to_string());
            syn::visit::visit_item_mod(self, item);
            self.module_path.pop();
        }
    }
}

fn collect_use_targets(tree: &UseTree, prefix: &mut Vec<String>, targets: &mut Vec<Vec<String>>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_use_targets(&path.tree, prefix, targets);
            prefix.pop();
        }
        UseTree::Name(name) => {
            let mut target = prefix.clone();
            target.push(name.ident.to_string());
            targets.push(target);
        }
        UseTree::Rename(rename) => {
            let mut target = prefix.clone();
            target.push(rename.ident.to_string());
            targets.push(target);
        }
        UseTree::Glob(_) => targets.push(prefix.clone()),
        UseTree::Group(group) => {
            for item in &group.items {
                collect_use_targets(item, prefix, targets);
            }
        }
    }
}

fn collect_root_alias_candidates(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    module_path: &[String],
    candidates: &mut Vec<RootAliasCandidate>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_root_alias_candidates(&path.tree, prefix, module_path, candidates);
            prefix.pop();
        }
        UseTree::Rename(rename) => {
            let mut source = prefix.clone();
            if rename.ident != "self" {
                source.push(rename.ident.to_string());
            }
            candidates.push(RootAliasCandidate {
                module_path: module_path.to_vec(),
                alias: rename.rename.to_string(),
                source,
            });
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_root_alias_candidates(item, prefix, module_path, candidates);
            }
        }
        UseTree::Name(_) | UseTree::Glob(_) => {}
    }
}

fn root_aliases(candidates: &[RootAliasCandidate]) -> RootAliases {
    let mut aliases = RootAliases::new();
    loop {
        let mut changed = false;
        for candidate in candidates {
            if resolve_path(&candidate.source, &candidate.module_path, &aliases)
                == ["crate".to_string()]
            {
                changed |= aliases.insert((candidate.module_path.clone(), candidate.alias.clone()));
            }
        }
        if !changed {
            return aliases;
        }
    }
}

fn resolve_path(
    segments: &[String],
    module_path: &[String],
    root_aliases: &RootAliases,
) -> Vec<String> {
    let Some(first) = segments.first().map(String::as_str) else {
        return Vec::new();
    };
    if first == "crate" {
        return segments.to_vec();
    }
    if root_aliases.contains(&(module_path.to_vec(), first.to_string())) {
        return std::iter::once("crate".to_string())
            .chain(segments.iter().skip(1).cloned())
            .collect();
    }

    let mut target = module_path.to_vec();
    let mut offset = 0;
    if first == "self" {
        offset = 1;
    } else {
        while segments
            .get(offset)
            .is_some_and(|segment| segment == "super")
        {
            if target.len() > 1 {
                target.pop();
            }
            offset += 1;
        }
        if offset == 0 {
            return segments.to_vec();
        }
    }
    target.extend(segments.iter().skip(offset).cloned());
    target
}

fn is_commands_dependency(segments: &[String]) -> bool {
    matches!(segments, [root, layer, ..] if root == "crate" && layer == "commands")
}

fn is_query_forbidden_dependency(segments: &[String]) -> bool {
    matches!(segments, [root, layer, ..]
        if root == "crate" && matches!(layer.as_str(), "commands" | "web" | "tui"))
}

fn expected_query_type_owner(name: &str) -> Option<&'static str> {
    match name {
        "Dashboard" => Some("mod.rs"),
        "OverviewPayload" | "TokenSummary" => Some("overview.rs"),
        "ModelBreakdown" | "SourceBreakdown" | "HostBreakdown" | "ProjectBreakdown" => {
            Some("breakdowns.rs")
        }
        "ActivityPayload" => Some("activity.rs"),
        "ToolsPayload" => Some("tools.rs"),
        "OptimizePayload" => Some("optimize.rs"),
        "ModelComparePayload" => Some("comparison.rs"),
        "DiagnosticsPayload" | "SyncCommandCenterPayload" => Some("diagnostics.rs"),
        "DashboardSnapshot" | "DashboardCoreSnapshot" | "DashboardInteractiveSnapshot" => {
            Some("snapshot.rs")
        }
        _ => None,
    }
}

fn expected_query_method_owner(name: &str) -> Option<&'static str> {
    match name {
        "open" | "open_with_busy_timeout" => Some("mod.rs"),
        "overview" | "trends" | "trends_daily" | "trends_hourly" | "trends_monthly" => {
            Some("overview.rs")
        }
        "model_breakdown" | "source_breakdown" | "host_breakdown" | "project_breakdown"
        | "cost_breakdown" | "context_pressure" => Some("breakdowns.rs"),
        "activity_breakdown" => Some("activity.rs"),
        "tool_breakdown" => Some("tools.rs"),
        "optimize" | "zombie_report" => Some("optimize.rs"),
        "compare_models" | "model_compare" => Some("comparison.rs"),
        "diagnostics" | "health" | "health_summary" | "sync_command_center" => {
            Some("diagnostics.rs")
        }
        "snapshot" | "core_snapshot" | "interactive_snapshot" => Some("snapshot.rs"),
        _ => None,
    }
}

fn query_ownership_violations_in(root: &Path) -> Result<Vec<String>, String> {
    let mut seen = std::collections::HashMap::<String, Vec<String>>::new();
    let mut violations = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("rs")
        {
            continue;
        }
        let file_name = entry
            .path()
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("non-UTF8 query path: {}", entry.path().display()))?;
        let source = std::fs::read_to_string(entry.path())
            .map_err(|error| format!("read {}: {error}", entry.path().display()))?;
        let syntax = syn::parse_file(&source)
            .map_err(|error| format!("parse {}: {error}", entry.path().display()))?;
        for item in syntax.items {
            if let Item::Struct(item) = &item
                && let Some(expected) = expected_query_type_owner(&item.ident.to_string())
            {
                let key = format!("type {}", item.ident);
                seen.entry(key.clone())
                    .or_default()
                    .push(file_name.to_string());
                if file_name != expected {
                    violations.push(format!("{key} belongs in {expected}, found in {file_name}"));
                }
            }
            let Item::Impl(item) = item else {
                continue;
            };
            let Type::Path(self_ty) = item.self_ty.as_ref() else {
                continue;
            };
            if self_ty
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
                != Some("Dashboard".to_string())
            {
                continue;
            }
            for impl_item in item.items {
                let ImplItem::Fn(method) = impl_item else {
                    continue;
                };
                let Some(expected) = expected_query_method_owner(&method.sig.ident.to_string())
                else {
                    continue;
                };
                let key = format!("method {}", method.sig.ident);
                seen.entry(key.clone())
                    .or_default()
                    .push(file_name.to_string());
                if file_name != expected {
                    violations.push(format!("{key} belongs in {expected}, found in {file_name}"));
                }
            }
        }
    }
    for (symbol, owners) in seen {
        if owners.len() != 1 {
            violations.push(format!(
                "{symbol} has {} implementations: {owners:?}",
                owners.len()
            ));
        }
    }
    Ok(violations)
}

fn is_command_sync_dependency(segments: &[String]) -> bool {
    matches!(segments, [root, commands, sync, ..]
        if root == "crate" && commands == "commands" && sync == "sync")
}

fn module_path_for_file(root: &Path, file: &Path, module_root: &[&str]) -> ModulePath {
    let mut module_path = module_root
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    let relative = if root.is_file() {
        PathBuf::from(file.file_name().unwrap_or_default())
    } else {
        file.strip_prefix(root).unwrap_or(file).to_path_buf()
    };
    if let Some(parent) = relative.parent() {
        module_path.extend(
            parent
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .filter(|segment| !segment.is_empty())
                .map(str::to_string),
        );
    }
    if relative.file_name().and_then(|name| name.to_str()) != Some("mod.rs")
        && let Some(stem) = relative.file_stem().and_then(|stem| stem.to_str())
    {
        module_path.push(stem.to_string());
    }
    module_path
}

fn violations_in(
    root: &Path,
    module_root: &[&str],
    forbidden: fn(&[String]) -> bool,
) -> Result<Vec<Violation>, String> {
    let mut violations = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("rs")
        {
            continue;
        }

        let source = std::fs::read_to_string(entry.path())
            .map_err(|error| format!("read {}: {error}", entry.path().display()))?;
        let syntax = syn::parse_file(&source)
            .map_err(|error| format!("parse {}: {error}", entry.path().display()))?;
        let module_path = module_path_for_file(root, entry.path(), module_root);
        let mut alias_collector = RootAliasCollector {
            module_path: module_path.clone(),
            candidates: Vec::new(),
        };
        alias_collector.visit_file(&syntax);
        let root_aliases = root_aliases(&alias_collector.candidates);
        let mut visitor = DependencyVisitor {
            file: entry.path(),
            module_path,
            root_aliases: &root_aliases,
            forbidden,
            violations: Vec::new(),
        };
        visitor.visit_file(&syntax);
        violations.extend(visitor.violations);
    }
    Ok(violations)
}

fn command_sync_violations_outside_commands() -> Result<Vec<Violation>, String> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    for entry in std::fs::read_dir(&src).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) == Some("commands") {
            continue;
        }
        if path.is_dir() {
            let module = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("non-UTF8 module path: {}", path.display()))?;
            violations.extend(violations_in(
                &path,
                &["crate", module],
                is_command_sync_dependency,
            )?);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
            violations.extend(violations_in(
                &path,
                &["crate"],
                is_command_sync_dependency,
            )?);
        }
    }
    Ok(violations)
}

#[derive(Debug)]
struct ImportBinding {
    module_path: ModulePath,
    local: String,
    target: Vec<String>,
}

struct ImportBindingCollector {
    module_path: ModulePath,
    bindings: Vec<ImportBinding>,
}

impl<'ast> Visit<'ast> for ImportBindingCollector {
    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        collect_import_bindings(
            &item.tree,
            &mut Vec::new(),
            &self.module_path,
            &mut self.bindings,
        );
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if item.content.is_some() {
            self.module_path.push(item.ident.to_string());
            syn::visit::visit_item_mod(self, item);
            self.module_path.pop();
        }
    }
}

fn collect_import_bindings(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    module_path: &[String],
    bindings: &mut Vec<ImportBinding>,
) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            collect_import_bindings(&path.tree, prefix, module_path, bindings);
            prefix.pop();
        }
        UseTree::Name(name) => {
            let mut target = prefix.clone();
            target.push(name.ident.to_string());
            bindings.push(ImportBinding {
                module_path: module_path.to_vec(),
                local: name.ident.to_string(),
                target,
            });
        }
        UseTree::Rename(rename) => {
            let mut target = prefix.clone();
            if rename.ident != "self" {
                target.push(rename.ident.to_string());
            }
            bindings.push(ImportBinding {
                module_path: module_path.to_vec(),
                local: rename.rename.to_string(),
                target,
            });
        }
        UseTree::Group(group) => {
            for item in &group.items {
                collect_import_bindings(item, prefix, module_path, bindings);
            }
        }
        UseTree::Glob(_) => {}
    }
}

struct SyncImplVisitor<'a> {
    file: &'a Path,
    module_path: ModulePath,
    root_aliases: &'a RootAliases,
    bindings: &'a [ImportBinding],
    violations: Vec<Violation>,
}

impl SyncImplVisitor<'_> {
    fn resolve_impl_path(&self, path: &SynPath) -> Vec<String> {
        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        let resolved = resolve_path(&segments, &self.module_path, self.root_aliases);
        if matches!(resolved.as_slice(), [root, sync, ..] if root == "crate" && sync == "sync") {
            return resolved;
        }
        let Some(first) = segments.first() else {
            return resolved;
        };
        let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| binding.module_path == self.module_path && binding.local == *first)
        else {
            return resolved;
        };
        let mut target = resolve_path(&binding.target, &self.module_path, self.root_aliases);
        target.extend(segments.into_iter().skip(1));
        target
    }

    fn record_path(&mut self, path: &SynPath) {
        let target = self.resolve_impl_path(path);
        if matches!(target.as_slice(), [root, sync, ..] if root == "crate" && sync == "sync") {
            self.violations.push(Violation {
                file: self.file.display().to_string(),
                line: path.span().start().line,
                target: target.join("::"),
            });
        }
    }
}

impl<'ast> Visit<'ast> for SyncImplVisitor<'_> {
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        if let Some((_, trait_path, _)) = &item.trait_ {
            self.record_path(trait_path);
        }
        if let Type::Path(type_path) = item.self_ty.as_ref() {
            self.record_path(&type_path.path);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        if item.content.is_some() {
            self.module_path.push(item.ident.to_string());
            syn::visit::visit_item_mod(self, item);
            self.module_path.pop();
        }
    }
}

fn sync_impl_violations_in(root: &Path, module_root: &[&str]) -> Result<Vec<Violation>, String> {
    let mut violations = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|error| error.to_string())?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|value| value.to_str()) != Some("rs")
        {
            continue;
        }
        let source = std::fs::read_to_string(entry.path())
            .map_err(|error| format!("read {}: {error}", entry.path().display()))?;
        let syntax = syn::parse_file(&source)
            .map_err(|error| format!("parse {}: {error}", entry.path().display()))?;
        let module_path = module_path_for_file(root, entry.path(), module_root);
        let mut alias_collector = RootAliasCollector {
            module_path: module_path.clone(),
            candidates: Vec::new(),
        };
        alias_collector.visit_file(&syntax);
        let root_aliases = root_aliases(&alias_collector.candidates);
        let mut binding_collector = ImportBindingCollector {
            module_path: module_path.clone(),
            bindings: Vec::new(),
        };
        binding_collector.visit_file(&syntax);
        let mut visitor = SyncImplVisitor {
            file: entry.path(),
            module_path,
            root_aliases: &root_aliases,
            bindings: &binding_collector.bindings,
            violations: Vec::new(),
        };
        visitor.visit_file(&syntax);
        violations.extend(visitor.violations);
    }
    Ok(violations)
}

#[test]
fn sync_layer_does_not_depend_on_commands() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sync");
    let violations =
        violations_in(&root, &["crate", "sync"], is_commands_dependency).expect("parse sync layer");
    assert!(
        violations.is_empty(),
        "ARCH-002 violations:\n{}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn remote_layer_does_not_depend_on_commands() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/remote");
    let violations = violations_in(&root, &["crate", "remote"], is_commands_dependency)
        .expect("parse remote layer");
    assert!(
        violations.is_empty(),
        "ARCH-002 remote violations:\n{}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn query_layer_does_not_depend_on_commands_web_or_tui() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query");
    let violations = violations_in(&root, &["crate", "query"], is_query_forbidden_dependency)
        .expect("parse query layer");
    assert!(
        violations.is_empty(),
        "ARCH-005 query dependency violations:\n{}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn query_vertical_module_ownership_is_canonical() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query");
    let violations = query_ownership_violations_in(&root).expect("scan query ownership");
    assert!(
        violations.is_empty(),
        "ARCH-006 query ownership violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn query_vertical_modules_stay_within_navigation_budgets() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/query");
    for (file, limit) in [
        ("mod.rs", 1_200_usize),
        ("overview.rs", 1_000),
        ("breakdowns.rs", 1_000),
        ("activity.rs", 1_000),
        ("tools.rs", 1_000),
        ("optimize.rs", 1_000),
        ("comparison.rs", 1_000),
        ("diagnostics.rs", 1_000),
        ("snapshot.rs", 1_000),
    ] {
        let source = std::fs::read_to_string(root.join(file))
            .unwrap_or_else(|error| panic!("read query module {file}: {error}"));
        let lines = source.lines().count();
        assert!(
            lines <= limit,
            "query module {file} has {lines} production lines; limit is {limit}"
        );
    }
}

#[test]
fn non_command_layers_do_not_depend_on_command_sync() {
    let violations = command_sync_violations_outside_commands().expect("scan non-command layers");
    assert!(
        violations.is_empty(),
        "ARCH-003 violations:\n{}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn command_layer_does_not_implement_sync_owned_types_or_traits() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    let violations = sync_impl_violations_in(&root, &["crate", "commands"])
        .expect("scan command impl ownership");
    assert!(
        violations.is_empty(),
        "ARCH-004 violations:\n{}",
        violations
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn fixtures_cover_supported_rust_path_forms() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/architecture/fixtures");
    let cases = [
        ("use_dependency.rs", "crate::commands::sync"),
        ("fully_qualified.rs", "crate::commands::sync::run_once"),
        ("alias.rs", "crate::commands"),
        ("nested_module.rs", "crate::commands::sync::run_once"),
        ("crate_alias.rs", "crate::commands::sync::run_once"),
        ("relative.rs", "crate::commands::sync::run_once"),
    ];

    for (name, expected_target) in cases {
        let violations = violations_in(
            &fixtures.join(name),
            &["crate", "sync"],
            is_commands_dependency,
        )
        .expect("parse violation fixture");
        let violation = violations
            .iter()
            .find(|violation| violation.target == expected_target)
            .unwrap_or_else(|| panic!("fixture {name}: {violations:?}"));
        assert!(violation.line > 0, "fixture {name} must report a line");
        assert!(
            violation.file.ends_with(name),
            "fixture {name} must report its source file: {violation:?}"
        );
    }

    let valid = violations_in(
        &fixtures.join("valid.rs"),
        &["crate", "sync"],
        is_commands_dependency,
    )
    .expect("parse valid fixture");
    assert!(valid.is_empty(), "valid fixture: {valid:?}");
}

#[test]
fn ownership_fixtures_cover_non_adapter_dependencies_and_sync_impls() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/architecture/fixtures");
    let adapter = violations_in(
        &fixtures.join("non_adapter_command_sync.rs"),
        &["crate", "web"],
        is_command_sync_dependency,
    )
    .expect("parse non-adapter fixture");
    assert!(
        adapter
            .iter()
            .any(|violation| violation.target == "crate::commands::sync")
    );

    for (name, expected) in [
        ("commands_impl_sync_trait.rs", "crate::sync::SyncExecutor"),
        ("commands_impl_sync_type.rs", "crate::sync::JobRegistry"),
        (
            "commands_impl_sync_alias.rs",
            "crate::sync::executor::SyncExecutor",
        ),
    ] {
        let violations = sync_impl_violations_in(&fixtures.join(name), &["crate", "commands"])
            .expect("parse impl ownership fixture");
        assert!(
            violations
                .iter()
                .any(|violation| violation.target == expected && violation.line > 0),
            "fixture {name}: {violations:?}"
        );
    }
}

#[test]
fn query_architecture_fixtures_cover_edges_and_canonical_ownership() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/architecture/fixtures");
    for (name, expected) in [
        ("query_forbidden_commands.rs", "crate::commands"),
        ("query_forbidden_web.rs", "crate::web"),
        ("query_forbidden_tui.rs", "crate::tui"),
    ] {
        let violations = violations_in(
            &fixtures.join(name),
            &["crate", "query"],
            is_query_forbidden_dependency,
        )
        .expect("parse query edge fixture");
        assert!(
            violations
                .iter()
                .any(|violation| violation.target == expected),
            "fixture {name}: {violations:?}"
        );
    }

    let valid = query_ownership_violations_in(&fixtures.join("query_valid"))
        .expect("parse valid query ownership fixture");
    assert!(valid.is_empty(), "valid query ownership fixture: {valid:?}");
    let invalid = query_ownership_violations_in(&fixtures.join("query_invalid"))
        .expect("parse invalid query ownership fixture");
    assert!(
        invalid
            .iter()
            .any(|violation| violation.contains("type OverviewPayload"))
            && invalid
                .iter()
                .any(|violation| violation.contains("method snapshot")),
        "invalid query ownership fixture: {invalid:?}"
    );
}
