//! The repos list projection: filter (search text, tag chip) + sort (mode,
//! favorites always floating to the top), optionally bucketed into
//! scan-root groups. Pure functions over the entry list, so ordering rules
//! are unit-testable without the store.

use devtools_core::paths::same_folder_text;
use devtools_core::repo_store::{RepoEntry, ARCHIVED_TAG, FAVORITES_TAG};
use devtools_core::settings::RepoSortMode;

/// One row of the table: either a repo entry or a collapsible scan-root
/// group header. Grouping is a pure projection — the header rows interleave
/// with the repo rows, so no per-row heights are needed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectedRow {
    Group {
        /// Stable identity for collapse/solo state: the scan-root path, or
        /// "other" for repos under no root.
        key: String,
        /// Display name: the root's last path segment ("Other" for other).
        name: String,
        /// Repos currently projected in the group (post-filter).
        count: usize,
        collapsed: bool,
    },
    Repo(usize),
}

impl ProjectedRow {
    /// The repo index when this row is a repo, None for group headers.
    pub fn repo_index(&self) -> Option<usize> {
        match self {
            ProjectedRow::Repo(ix) => Some(*ix),
            ProjectedRow::Group { .. } => None,
        }
    }
}

/// Grouping inputs: the scan roots to bucket by, the persisted collapsed
/// keys, and the session-only solo filter. A `None` grouping keeps the
/// historical flat list (repo rows only, no headers).
pub struct Grouping<'a> {
    pub roots: &'a [String],
    pub collapsed: &'a std::collections::HashSet<String>,
    /// When set, only this group's repos are projected (the header's
    /// solo-filter icon; the toolbar's Clear removes it).
    pub solo: Option<&'a str>,
}

/// The fallback group for repos under no scan root.
pub const OTHER_GROUP_KEY: &str = "other";
/// Display name of the fallback group.
pub const OTHER_GROUP_NAME: &str = "Other";

pub fn project(
    repos: &[RepoEntry],
    checked_tags: &[String],
    sort: RepoSortMode,
    ascending: bool,
    show_archived: bool,
    grouping: Option<&Grouping>,
) -> Vec<ProjectedRow> {
    // One shared sort for both shapes: the comparators (favorites first,
    // archived last, mode order, name tiebreak) are identical, so grouping
    // only re-buckets the sorted rows — a group's repos keep the flat order.
    let sorted = sort_indices(repos, checked_tags, sort, ascending, show_archived);
    let Some(grouping) = grouping else {
        return sorted.into_iter().map(ProjectedRow::Repo).collect();
    };

    // OR-tag filter first (as flat), then the solo filter: only the soloed
    // group's repos survive, the rest of the buckets come up empty.
    let solo = grouping.solo;
    let mut groups: Vec<(String, String, Vec<usize>)> = Vec::new();
    for &ix in &sorted {
        let (key, name) = assign_group(&repos[ix].folder_path.to_string_lossy(), grouping.roots);
        if solo.is_some_and(|solo| !keys_match(solo, &key)) {
            continue;
        }
        let bucket = groups
            .iter_mut()
            .find(|(bucket_key, _, _)| keys_match(bucket_key, &key));
        match bucket {
            Some((_, _, rows)) => rows.push(ix),
            None => groups.push((key, name, vec![ix])),
        }
    }

    // "other" sorts last regardless of the roots' order; root buckets sort
    // by key so the projection stays deterministic even when the settings
    // file lists the roots unsorted.
    groups.sort_by_key(|(key, _, _)| key.to_lowercase());
    let other = groups
        .iter()
        .position(|(key, _, _)| key == OTHER_GROUP_KEY)
        .map(|ix| groups.remove(ix));
    let mut rows = Vec::new();
    for (key, name, bucket) in &groups {
        push_group(&mut rows, key, name, bucket, grouping.collapsed);
    }
    if let Some((key, name, bucket)) = other {
        push_group(&mut rows, &key, &name, &bucket, grouping.collapsed);
    }
    rows
}

fn push_group(
    rows: &mut Vec<ProjectedRow>,
    key: &str,
    name: &str,
    bucket: &[usize],
    collapsed: &std::collections::HashSet<String>,
) {
    // empty buckets never happen post-filter, but a soloed-away group must
    // not leave a header behind either — drop them here, uniformly
    if bucket.is_empty() {
        return;
    }
    let is_collapsed = collapsed.iter().any(|k| keys_match(k, key));
    rows.push(ProjectedRow::Group {
        key: key.to_string(),
        name: name.to_string(),
        count: bucket.len(),
        collapsed: is_collapsed,
    });
    if is_collapsed {
        return;
    }
    rows.extend(bucket.iter().copied().map(ProjectedRow::Repo));
}

fn keys_match(a: &str, b: &str) -> bool {
    a == b
}

/// Longest-prefix assignment: a repo lands under the deepest root that
/// contains its folder (roots may themselves be repo folders), so nested
/// roots are deterministic. No match → the "other" fallback group.
fn assign_group(folder: &str, roots: &[String]) -> (String, String) {
    let mut best: Option<&String> = None;
    for root in roots {
        if !is_under(folder, root) {
            continue;
        }
        let longer = match best {
            None => true,
            Some(current) => {
                root.trim_end_matches(['/', '\\']).chars().count()
                    > current.trim_end_matches(['/', '\\']).chars().count()
            }
        };
        if longer {
            best = Some(root);
        }
    }
    match best {
        Some(root) => (root.clone(), last_path_segment(root)),
        None => (OTHER_GROUP_KEY.to_string(), OTHER_GROUP_NAME.to_string()),
    }
}

/// Folder identity with prefix semantics: equal, or below the root through a
/// path separator. Roots pointing at repo folders match themselves.
fn is_under(folder: &str, root: &str) -> bool {
    let root = root.trim_end_matches(['/', '\\']);
    if root.is_empty() {
        return false;
    }
    let folder = folder.trim_end_matches(['/', '\\']);
    if same_folder_text(folder, root) {
        return true;
    }
    // get() keeps a non-char-boundary slice from panicking on odd paths
    match folder.get(..root.len()) {
        Some(prefix) if folder.len() > root.len() => {
            same_folder_text(prefix, root)
                && (folder.as_bytes()[root.len()] == b'/' || folder.as_bytes()[root.len()] == b'\\')
        }
        _ => false,
    }
}

fn last_path_segment(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// The flat pipeline: archived exclusion → OR-tag filter → sort. Returns
/// indices into `repos`; grouping re-buckets these without re-sorting.
fn sort_indices(
    repos: &[RepoEntry],
    checked_tags: &[String],
    sort: RepoSortMode,
    ascending: bool,
    show_archived: bool,
) -> Vec<usize> {
    let mut rows: Vec<usize> = repos
        .iter()
        .enumerate()
        // archived repos leave every view unless explicitly shown; the
        // exclusion is independent of the OR-tag filter so an archived repo
        // never satisfies a checked chip
        .filter(|(_, repo)| show_archived || !repo.is_archived())
        .filter(|(_, repo)| {
            // OR semantics: a repo matches when it carries any checked tag
            checked_tags.is_empty()
                || checked_tags.iter().any(|t| {
                    t.eq_ignore_ascii_case(FAVORITES_TAG) && repo.is_favorite()
                        || repo.tags.iter().any(|rt| rt.eq_ignore_ascii_case(t))
                })
        })
        .map(|(ix, _)| ix)
        .collect();

    // case-insensitive name order needs the lowercase form twice (the Name
    // mode and the universal tiebreaker); precompute once instead of two
    // allocations per comparison
    let names_lower: Vec<String> = repos.iter().map(|repo| repo.name.to_lowercase()).collect();
    let name_order = |ascending: bool, a: usize, b: usize| {
        if ascending {
            names_lower[a].cmp(&names_lower[b])
        } else {
            names_lower[b].cmp(&names_lower[a])
        }
    };

    rows.sort_by(|&a, &b| {
        let (ra, rb) = (&repos[a], &repos[b]);
        // archived repos sink below every live one when shown (the archived
        // partition is empty otherwise, so this is a no-op then); favorites
        // float within their partition; then the mode's order — `ascending`
        // flips the comparator around the mode's default; name breaks ties.
        // Repos without a commit date sink in BOTH LastActivity directions.
        ra.is_archived()
            .cmp(&rb.is_archived())
            .then_with(|| rb.is_favorite().cmp(&ra.is_favorite()))
            .then_with(|| match sort {
                RepoSortMode::Name => name_order(ascending, a, b),
                RepoSortMode::LastActivity => {
                    order_opt(ascending, ra.git_last_commit_at, rb.git_last_commit_at)
                }
                RepoSortMode::Branch => {
                    order_opt(ascending, ra.git_branch_name.as_deref(), rb.git_branch_name.as_deref())
                }
                RepoSortMode::Changes => order_key(ascending, changes_weight(ra), changes_weight(rb)),
                RepoSortMode::GitHub => order_key(
                    ascending,
                    ra.github_pr_count + ra.github_issue_count,
                    rb.github_pr_count + rb.github_issue_count,
                ),
            })
            .then_with(|| names_lower[a].cmp(&names_lower[b]))
    });
    rows
}

/// Options order by their value, but `None` sinks regardless of direction.
fn order_opt<T: Ord + Copy>(
    ascending: bool,
    a: Option<T>,
    b: Option<T>,
) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a), Some(b)) => order_key(ascending, a, b),
    }
}

fn order_key<T: Ord + Copy>(ascending: bool, a: T, b: T) -> std::cmp::Ordering {
    if ascending { a.cmp(&b) } else { b.cmp(&a) }
}

fn changes_weight(repo: &RepoEntry) -> u32 {
    repo.git_modified_count * 10 + repo.git_to_push_count * 5 + repo.git_to_pull_count
}

/// Every distinct tag across the list; favorites and platform always lead.
/// The archived marker is a mechanism, not a filter chip — it never lists.
pub fn all_tags(repos: &[RepoEntry]) -> Vec<String> {
    let mut tags: Vec<String> = vec![FAVORITES_TAG.to_string(), devtools_core::scan::PLATFORM_TAG.to_string()];
    for repo in repos {
        for tag in &repo.tags {
            if tag.eq_ignore_ascii_case(ARCHIVED_TAG) {
                continue;
            }
            if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                tags.push(tag.clone());
            }
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo(name: &str, path: &str, tags: &[&str]) -> RepoEntry {
        RepoEntry {
            name: name.into(),
            folder_path: PathBuf::from(path),
            solution_path: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            git_branch_name: None,
            git_modified_count: 0,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: false,
            git_branch_published: false,
            git_status_loaded: false,
            github_pr_count: 0,
            github_issue_count: 0,
        }
    }

    fn repos() -> Vec<RepoEntry> {
        vec![
            repo("alpha", "/x/alpha", &["favorites"]),
            repo("beta", "/x/beta", &["work"]),
            repo("gamma", "/x/gamma", &[]),
        ]
    }

    /// The flat projection (grouping off), as the tests below read it.
    fn flat(repos: &[RepoEntry], tags: &[String], mode: RepoSortMode, asc: bool, archived: bool) -> Vec<ProjectedRow> {
        project(repos, tags, mode, asc, archived, None)
    }

    /// Names of the projected repo rows; panics on an unexpected header so
    /// the flat-mode assertions fail loudly when grouping leaks in.
    fn repo_names<'a>(rows: &[ProjectedRow], all: &'a [RepoEntry]) -> Vec<&'a str> {
        rows.iter()
            .map(|row| match row {
                ProjectedRow::Repo(ix) => all[*ix].name.as_str(),
                ProjectedRow::Group { .. } => panic!("unexpected group header in a flat projection"),
            })
            .collect()
    }

    /// A compact read of a grouped projection for assertions: header keys
    /// prefixed "#", repo names as-is.
    fn sketch(rows: &[ProjectedRow], all: &[RepoEntry]) -> Vec<String> {
        rows.iter()
            .map(|row| match row {
                ProjectedRow::Repo(ix) => all[*ix].name.to_string(),
                ProjectedRow::Group { key, count, .. } => format!("#{key}({count})"),
            })
            .collect()
    }

    fn grouping<'a>(roots: &'a [String], collapsed: &'a std::collections::HashSet<String>) -> Grouping<'a> {
        Grouping { roots, collapsed, solo: None }
    }

    #[test]
    fn favorites_float_and_name_sorts() {
        let all = repos();
        let rows = flat(&all, &[], RepoSortMode::Name, true, false);
        assert_eq!(repo_names(&rows, &all), vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn tag_filter_favorites_and_user_tags() {
        let all = repos();
        let rows = flat(&all, &["favorites".to_string()], RepoSortMode::Name, true, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0].repo_index().unwrap()].name, "alpha");

        let rows = flat(&all, &["work".to_string()], RepoSortMode::Name, true, false);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0].repo_index().unwrap()].name, "beta");
    }

    #[test]
    fn checked_tags_union_their_repos() {
        let all = repos();
        let rows = flat(
            &all,
            &["favorites".to_string(), "work".to_string()],
            RepoSortMode::Name,
            true,
            false,
        );
        assert_eq!(rows.len(), 2);
        let names: Vec<&str> = rows
            .iter()
            .map(|row| all[row.repo_index().unwrap()].name.as_str())
            .collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn all_tags_leads_with_favorites_then_platform() {
        let mut all = repos();
        all[1].tags.push("platform".into());
        let tags = all_tags(&all);
        assert_eq!(tags[0], "favorites");
        assert_eq!(tags[1], "platform");
        assert!(tags.contains(&"work".to_string()));
    }

    #[test]
    fn name_direction_flips_and_favorites_still_float() {
        let all = repos();
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Name, true, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
        // alpha is a favorite, so it leads in both directions
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Name, false, false), &all),
            vec!["alpha", "gamma", "beta"]
        );
    }

    #[test]
    fn last_activity_flips_but_unknown_dates_stay_last() {
        let mut all = repos();
        all[0].tags.clear();
        all[0].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(2));
        all[1].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(1));
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::LastActivity, false, false), &all),
            vec!["beta", "alpha", "gamma"],
            "descending leads with the most recent commit"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::LastActivity, true, false), &all),
            vec!["alpha", "beta", "gamma"],
            "ascending leads with the oldest commit; no date still sinks"
        );
    }

    #[test]
    fn changes_direction_flips_best_first() {
        let mut all = repos();
        all[0].tags.clear();
        all[0].git_modified_count = 5;
        all[1].git_modified_count = 2;
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Changes, false, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Changes, true, false), &all),
            vec!["gamma", "beta", "alpha"]
        );
    }

    #[test]
    fn branch_sorts_alphabetically_and_unknown_sinks() {
        let mut all = repos();
        all[0].tags.clear();
        all[0].git_branch_name = Some("zeta".into());
        all[1].git_branch_name = Some("alpha".into());
        // gamma stays on the entry default ("main" in most setups); give it a
        // distinct middle value here
        all[2].git_branch_name = Some("main".into());
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, true, false), &all),
            vec!["beta", "gamma", "alpha"],
            "ascending is alphabetical by branch"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, false, false), &all),
            vec!["alpha", "gamma", "beta"],
            "descending flips the branch order"
        );

        all[2].git_branch_name = None;
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, true, false), &all),
            vec!["beta", "alpha", "gamma"],
            "a repo without a branch sinks in both directions"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::Branch, false, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
    }

    #[test]
    fn github_sort_sums_prs_and_issues() {
        let mut all = repos();
        all[0].tags.clear();
        all[1].github_pr_count = 3;
        all[2].github_issue_count = 4;
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::GitHub, false, false), &all),
            vec!["gamma", "beta", "alpha"],
            "descending leads with the most open PRs + issues"
        );
        assert_eq!(
            repo_names(&flat(&all, &[], RepoSortMode::GitHub, true, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
    }

    #[test]
    fn archived_repos_hide_by_default_and_sink_when_shown() {
        let mut all = repos();
        all[1].tags.push("archived".into());
        let names = |show: bool| -> Vec<&str> {
            repo_names(&flat(&all, &[], RepoSortMode::Name, true, show), &all)
        };
        assert_eq!(names(false), vec!["alpha", "gamma"], "the archived repo hides by default");
        assert_eq!(
            names(true),
            vec!["alpha", "gamma", "beta"],
            "shown archived repos sort below every live repo"
        );
    }

    #[test]
    fn archived_partition_keeps_favorites_first_within_itself() {
        let mut all = repos();
        // alpha: live favorite; beta: archived favorite; gamma: plain archived
        all[1].tags = vec!["archived".into(), "favorites".into()];
        all[2].tags = vec!["archived".into()];
        all[0].git_branch_name = Some("zeta".into());
        let rows = flat(&all, &[], RepoSortMode::Branch, true, true);
        assert_eq!(repo_names(&rows, &all), vec!["alpha", "beta", "gamma"],
            "live repos first, then archived favorites, then the rest");
    }

    #[test]
    fn archived_repo_matching_a_checked_tag_stays_hidden() {
        let mut all = repos();
        all[1].tags.push("work".into());
        all[1].tags.push("archived".into());
        let rows = flat(&all, &["work".to_string()], RepoSortMode::Name, true, false);
        assert!(
            rows.is_empty(),
            "the archived flag excludes before the OR-tag filter"
        );
        let rows = flat(&all, &["work".to_string()], RepoSortMode::Name, true, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0].repo_index().unwrap()].name, "beta");
    }

    #[test]
    fn all_tags_never_lists_the_archived_marker() {
        let mut all = repos();
        all[1].tags.push("archived".into());
        all[1].tags.push("work".into());
        let tags = all_tags(&all);
        assert!(
            !tags.iter().any(|t| t.eq_ignore_ascii_case("archived")),
            "archived is a mechanism, not a filter chip"
        );
        assert!(tags.contains(&"work".to_string()));
    }

    // ---- grouping ----------------------------------------------------------

    fn grouped_repos() -> Vec<RepoEntry> {
        vec![
            repo("alpha", "/roots/a/alpha", &[]),
            repo("beta", "/roots/a/beta", &[]),
            repo("gamma", "/roots/b/gamma", &[]),
            repo("delta", "/elsewhere/delta", &[]),
        ]
    }

    #[test]
    fn repos_group_by_longest_matching_root() {
        let all = vec![
            repo("alpha", "/work/alpha", &[]),
            repo("beta", "/work/rust/beta", &[]),
            repo("gamma", "/work/rust/deep/gamma", &[]),
        ];
        let roots = vec!["/work".to_string(), "/work/rust".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&grouping(&roots, &collapsed)));
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/work(1)", "alpha", "#/work/rust(2)", "beta", "gamma"],
            "the deeper root claims the nested repos; headers interleave in root order"
        );
        let ProjectedRow::Group { name, count, collapsed: open, .. } = &rows[2] else {
            panic!("expected a group header");
        };
        assert_eq!(name, "rust", "the header names itself after the root's last segment");
        assert_eq!(*count, 2);
        assert!(!open, "groups start expanded");
    }

    #[test]
    fn repos_under_no_root_land_in_other() {
        let all = grouped_repos();
        let roots = vec!["/roots/a".to_string(), "/roots/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&grouping(&roots, &collapsed)));
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(2)", "alpha", "beta", "#/roots/b(1)", "gamma", "#other(1)", "delta"],
            "the fallback group sorts last and names itself Other"
        );
        let ProjectedRow::Group { key, name, .. } = &rows[5] else {
            panic!("expected the other-group header");
        };
        assert_eq!(key, "other");
        assert_eq!(name, "Other");
    }

    #[test]
    fn an_empty_other_group_leaves_no_header() {
        let mut all = grouped_repos();
        all[3].folder_path = PathBuf::from("/roots/b/moved");
        let roots = vec!["/roots/a".to_string(), "/roots/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&grouping(&roots, &collapsed)));
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(2)", "alpha", "beta", "#/roots/b(2)", "delta", "gamma"],
            "every repo sits under a root, so no fallback header appears; repos sort by name within the group"
        );
    }

    #[test]
    fn grouping_disabled_matches_the_flat_projection() {
        let mut all = repos();
        all[1].tags.push("archived".into());
        let rows = project(&all, &[], RepoSortMode::Name, true, true, None);
        assert_eq!(
            repo_names(&rows, &all),
            vec!["alpha", "gamma", "beta"],
            "grouping off reproduces the archived-last flat order with no headers"
        );
    }

    #[test]
    fn collapsing_a_group_leaves_only_its_header() {
        let all = grouped_repos();
        let roots = vec!["/roots/a".to_string(), "/roots/b".to_string()];
        let collapsed: std::collections::HashSet<String> = ["/roots/a".to_string()].into();
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&grouping(&roots, &collapsed)));
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(2)", "#/roots/b(1)", "gamma", "#other(1)", "delta"],
            "the collapsed group keeps its header and count but drops its repo rows"
        );
    }

    #[test]
    fn tag_filter_composes_with_groups_and_drops_empty_ones() {
        let mut all = grouped_repos();
        all[0].tags.push("work".into()); // alpha only
        let roots = vec!["/roots/a".to_string(), "/roots/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(
            &all,
            &["work".to_string()],
            RepoSortMode::Name,
            true,
            false,
            Some(&grouping(&roots, &collapsed)),
        );
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(1)", "alpha"],
            "the OR-tag filter applies per repo; groups without matches vanish"
        );
    }

    #[test]
    fn archived_partition_stays_within_its_group() {
        let mut all = grouped_repos();
        all[1].tags.push("archived".into()); // beta archives under /roots/a
        all[3].tags.push("archived".into()); // delta archives under other
        let roots = vec!["/roots/a".to_string(), "/roots/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(&all, &[], RepoSortMode::Name, true, true, Some(&grouping(&roots, &collapsed)));
        assert_eq!(
            sketch(&rows, &all),
            vec![
                "#/roots/a(2)", "alpha", "beta",
                "#/roots/b(1)", "gamma",
                "#other(1)", "delta",
            ],
            "shown archived repos sink within their own group (beta stays under /roots/a instead of dropping below gamma)"
        );
    }

    #[test]
    fn solo_filter_keeps_only_that_group() {
        let all = grouped_repos();
        let roots = vec!["/roots/a".to_string(), "/roots/b".to_string()];
        let collapsed = std::collections::HashSet::new();
        let solo = Grouping { roots: &roots, collapsed: &collapsed, solo: Some("/roots/a") };
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&solo));
        assert_eq!(
            sketch(&rows, &all),
            vec!["#/roots/a(2)", "alpha", "beta"],
            "soloing one group hides the other groups entirely"
        );

        let solo_other = Grouping { roots: &roots, collapsed: &collapsed, solo: Some("other") };
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&solo_other));
        assert_eq!(sketch(&rows, &all), vec!["#other(1)", "delta"]);
    }

    #[test]
    fn roots_may_point_at_repo_folders_themselves() {
        // scan roots are repo folders too (the adopt/add flow persists them),
        // so a root must match its own folder, not only subfolders
        let all = vec![repo("alpha", "/repos/alpha", &[])];
        let roots = vec!["/repos/alpha".to_string()];
        let collapsed = std::collections::HashSet::new();
        let rows = project(&all, &[], RepoSortMode::Name, true, false, Some(&grouping(&roots, &collapsed)));
        assert_eq!(sketch(&rows, &all), vec!["#/repos/alpha(1)", "alpha"]);
    }
}
