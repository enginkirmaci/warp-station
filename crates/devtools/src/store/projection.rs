//! The repos list projection: filter (search text, tag chip) + sort (mode,
//! favorites always floating to the top). Pure functions over the entry list,
//! so ordering rules are unit-testable without the store.

use devtools_core::repo_store::{RepoEntry, FAVORITES_TAG};
use devtools_core::settings::RepoSortMode;

pub fn project(
    repos: &[RepoEntry],
    checked_tags: &[String],
    sort: RepoSortMode,
    ascending: bool,
) -> Vec<usize> {
    let mut rows: Vec<usize> = repos
        .iter()
        .enumerate()
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
        // favorites first; then the mode's order — `ascending` flips the
        // comparator around the mode's default; name breaks ties. Repos
        // without a commit date sink in BOTH LastActivity directions.
        rb.is_favorite()
            .cmp(&ra.is_favorite())
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
pub fn all_tags(repos: &[RepoEntry]) -> Vec<String> {
    let mut tags: Vec<String> = vec![FAVORITES_TAG.to_string(), devtools_core::scan::PLATFORM_TAG.to_string()];
    for repo in repos {
        for tag in &repo.tags {
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

    #[test]
    fn favorites_float_and_name_sorts() {
        let all = repos();
        let rows = project(&all, &[], RepoSortMode::Name, true);
        let names: Vec<&str> = rows.iter().map(|&i| all[i].name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn tag_filter_favorites_and_user_tags() {
        let all = repos();
        let rows = project(&all, &["favorites".to_string()], RepoSortMode::Name, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0]].name, "alpha");

        let rows = project(&all, &["work".to_string()], RepoSortMode::Name, true);
        assert_eq!(rows.len(), 1);
        assert_eq!(all[rows[0]].name, "beta");
    }

    #[test]
    fn checked_tags_union_their_repos() {
        let all = repos();
        let rows = project(
            &all,
            &["favorites".to_string(), "work".to_string()],
            RepoSortMode::Name,
            true,
        );
        assert_eq!(rows.len(), 2);
        let names: Vec<&str> = rows.iter().map(|&i| all[i].name.as_str()).collect();
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
        let names = |rows: &[usize]| -> Vec<&str> {
            rows.iter().map(|&i| all[i].name.as_str()).collect()
        };
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Name, true)),
            vec!["alpha", "beta", "gamma"]
        );
        // alpha is a favorite, so it leads in both directions
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Name, false)),
            vec!["alpha", "gamma", "beta"]
        );
    }

    #[test]
    fn last_activity_flips_but_unknown_dates_stay_last() {
        let mut all = repos();
        all[0].tags.clear();
        all[0].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(2));
        all[1].git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(1));
        let names = |rows: &[usize]| -> Vec<&str> {
            rows.iter().map(|&i| all[i].name.as_str()).collect()
        };
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::LastActivity, false)),
            vec!["beta", "alpha", "gamma"],
            "descending leads with the most recent commit"
        );
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::LastActivity, true)),
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
        let names = |rows: &[usize]| -> Vec<&str> {
            rows.iter().map(|&i| all[i].name.as_str()).collect()
        };
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Changes, false)),
            vec!["alpha", "beta", "gamma"]
        );
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Changes, true)),
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
        fn names<'a>(rows: &[usize], all: &'a [RepoEntry]) -> Vec<&'a str> {
            rows.iter().map(|&i| all[i].name.as_str()).collect()
        }
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Branch, true), &all),
            vec!["beta", "gamma", "alpha"],
            "ascending is alphabetical by branch"
        );
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Branch, false), &all),
            vec!["alpha", "gamma", "beta"],
            "descending flips the branch order"
        );

        all[2].git_branch_name = None;
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Branch, true), &all),
            vec!["beta", "alpha", "gamma"],
            "a repo without a branch sinks in both directions"
        );
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::Branch, false), &all),
            vec!["alpha", "beta", "gamma"]
        );
    }

    #[test]
    fn github_sort_sums_prs_and_issues() {
        let mut all = repos();
        all[0].tags.clear();
        all[1].github_pr_count = 3;
        all[2].github_issue_count = 4;
        let names = |rows: &[usize]| -> Vec<&str> {
            rows.iter().map(|&i| all[i].name.as_str()).collect()
        };
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::GitHub, false)),
            vec!["gamma", "beta", "alpha"],
            "descending leads with the most open PRs + issues"
        );
        assert_eq!(
            names(&project(&all, &[], RepoSortMode::GitHub, true)),
            vec!["alpha", "beta", "gamma"]
        );
    }
}
