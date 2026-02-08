use crate::commands::scan;
use crate::exclude_file::{ensure_exclude_file_for_write, normalize_entry, ExcludeFile};
use crate::git;
use crate::git::RepoContext;
use crate::patterns::PatternCategory;
use crate::ui;
use anyhow::{anyhow, Result};
use dialoguer::MultiSelect;
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct AddSummary {
    pub added: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone)]
struct InteractiveCandidate {
    path: String,
    category: &'static str,
}

pub fn run(files: Vec<String>, interactive: bool, dry_run: bool) -> Result<i32> {
    let ctx = git::ensure_repo()?;
    let mut exclude = ensure_exclude_file_for_write(&ctx.exclude_path)?;

    if interactive || (files.is_empty() && ui::is_stdout_tty()) {
        return run_interactive(&ctx, &mut exclude, dry_run);
    }

    if files.is_empty() {
        return Err(anyhow!("no files provided. Use 'layer add <files...>' or run in a terminal for interactive mode"));
    }

    let summary = apply_add_entries(&ctx, &mut exclude, &files, dry_run)?;
    if dry_run {
        ui::print_dry_run_notice();
    }
    if summary.added == 0 {
        return Ok(2);
    }

    Ok(0)
}

pub fn apply_add_entries(
    ctx: &RepoContext,
    exclude: &mut ExcludeFile,
    entries: &[String],
    dry_run: bool,
) -> Result<AddSummary> {
    let mut summary = AddSummary::default();
    let mut known_entries = exclude.entry_set();

    for raw in entries {
        let normalized = normalize_entry(raw);
        if normalized.is_empty() {
            summary.skipped += 1;
            continue;
        }

        if known_entries.contains(&normalized) {
            println!("  {} '{normalized}' already layered", ui::info());
            summary.skipped += 1;
            continue;
        }

        if git::is_tracked(&ctx.root, &normalized)? {
            ui::print_warning(&format!("'{normalized}' is tracked by Git — layering won't hide it until untracked"));
            println!("  {}", ui::warn_text(&format!("git rm --cached {normalized}")));
        }

        if dry_run {
            println!("  {} Would layer '{normalized}'", ui::discovered());
        } else {
            exclude.append_entry(&normalized);
            println!("  {} Layered '{normalized}'", ui::ok());
        }
        known_entries.insert(normalized);
        summary.added += 1;
    }

    if summary.added > 0 && !dry_run {
        exclude.write(&ctx.exclude_path)?;
    }

    Ok(summary)
}

fn run_interactive(ctx: &RepoContext, exclude: &mut ExcludeFile, dry_run: bool) -> Result<i32> {
    ui::require_tty("interactive mode requires a TTY. Use 'layer add <files...>' instead")?;

    let candidates = collect_candidates(ctx, exclude)?;
    if candidates.is_empty() {
        println!("No context files found.");
        return Ok(2);
    }

    let displays: Vec<String> = candidates
        .iter()
        .map(|c| format!("{} {}", c.path, ui::dim_text(&format!("({})", c.category))))
        .collect();

    println!("{}", ui::heading("Select files to add to your local layer"));
    let theme = ui::layer_theme();
    ui::print_select_hint();
    let selections = MultiSelect::with_theme(&theme)
        .items(&displays)
        .report(false)
        .interact_opt()?;

    let selected = selections.unwrap_or_default();
    if selected.is_empty() {
        println!("No files selected.");
        return Ok(2);
    }

    let chosen = selected
        .into_iter()
        .map(|idx| candidates[idx].path.clone())
        .collect::<Vec<_>>();

    let summary = apply_add_entries(ctx, exclude, &chosen, dry_run)?;
    if dry_run {
        ui::print_dry_run_notice();
    }
    if summary.added == 0 {
        return Ok(2);
    }

    Ok(0)
}

fn collect_candidates(ctx: &RepoContext, exclude: &ExcludeFile) -> Result<Vec<InteractiveCandidate>> {
    let excluded = exclude.entry_set();
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for found in scan::discover_known_files(ctx, &excluded)? {
        if found.already_excluded || found.is_gitignored || found.is_tracked {
            continue;
        }
        if seen.insert(found.path.clone()) {
            let category = match found.category {
                PatternCategory::AiConfig => "context file",
            };
            out.push(InteractiveCandidate {
                path: found.path,
                category,
            });
        }
    }

    for file in git::list_untracked(&ctx.root)? {
        let normalized = normalize_entry(&file);
        if normalized.is_empty() || excluded.contains(&normalized) {
            continue;
        }
        if seen.insert(normalized.clone()) {
            out.push(InteractiveCandidate {
                path: normalized,
                category: "untracked",
            });
        }
    }

    Ok(out)
}
