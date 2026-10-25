#![allow(clippy::expect_used, clippy::missing_panics_doc)]

use std::borrow::Cow;
use std::sync::LazyLock;

use foldhash::HashSet;

use mago_allocator::LocalArena;
use mago_analyzer::Analyzer;
use mago_analyzer::analysis_result::AnalysisResult;
use mago_analyzer::plugin::PluginRegistry;
use mago_analyzer::settings::Settings;
use mago_codex::populator::populate_codebase;
use mago_codex::scanner::scan_program;
use mago_database::DatabaseReader;
use mago_database::file::File;
use mago_names::resolver::NameResolver;
use mago_php_version::PHPVersion;
use mago_prelude::Prelude;
use mago_syntax::parser::parse_file;
use mago_word::WordSet;

static PRELUDE: LazyLock<Prelude> = LazyLock::new(Prelude::build);

const SOURCE: &str = "<?php

declare(strict_types=1);

class Statement
{
    public function execute(): void {}
}

function run(Statement|false $statement): void
{
    $statement->execute();
}
";

#[test]
fn invalid_method_access_annotation_reports_the_full_expression_type() {
    let Prelude { mut database, mut metadata, mut symbol_references } = PRELUDE.clone();
    let file = File::ephemeral(Cow::Borrowed(&b"invalid-method-access.php"[..]), Cow::Borrowed(SOURCE.as_bytes()));
    let file_id = database.add(file);
    let source_file = database.get_ref(&file_id).expect("file just added should exist");

    let arena = LocalArena::new();
    let program = parse_file(&arena, source_file);
    assert!(!program.has_errors(), "the fixture should parse");
    let resolved_names = NameResolver::new(&arena).resolve(program);
    let settings = Settings::new(PHPVersion::PHP85);
    metadata.extend(scan_program(&arena, source_file, program, &resolved_names, settings.version));
    populate_codebase(&mut metadata, &mut symbol_references, WordSet::default(), HashSet::default());

    let registry = PluginRegistry::with_library_providers();
    let mut result = AnalysisResult::new(symbol_references);
    Analyzer::new(&arena, source_file, &resolved_names, &metadata, &registry, settings)
        .analyze(program, &mut result)
        .expect("analysis failed");

    let issue = result
        .issues
        .iter()
        .find(|issue| issue.code.as_deref() == Some("invalid-method-access"))
        .expect("expected an invalid-method-access issue");

    assert!(
        issue.message.contains("(`false`)"),
        "the main message should name the offending atomic type: {}",
        issue.message
    );
    let messages = issue.annotations.iter().filter_map(|a| a.message.as_deref()).collect::<Vec<_>>();
    assert!(
        messages.iter().any(|m| m.contains("This expression has type `Statement|false`")),
        "the annotation should name the full expression type: {messages:?}"
    );
}
