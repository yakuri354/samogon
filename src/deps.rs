use anyhow::{anyhow, Context, Result};
use std::collections::HashSet;

use crate::repo::Repo;

pub fn find_deps(pkgs: &[String], repo: &Repo) -> Result<Vec<String>> {
    // DFS and topological sort of the dependency DAG

    let mut deps = Vec::new();
    let mut visited = HashSet::new();

    for p in pkgs {
        if !visited.contains(p) {
            find_deps_dfs(p, repo, &mut deps, &mut visited)?;
        }
    }

    Ok(deps)
}

fn find_deps_dfs(
    package: &str,
    repo: &Repo,
    deps: &mut Vec<String>,
    visited: &mut HashSet<String>,
) -> Result<()> {
    visited.insert(package.to_string());

    let formula = repo.formulae.get(package).context(anyhow!(
        "Nonexistent package {} listed as a dependency",
        package
    ))?;

    for dep in formula.deps.iter() {
        // TODO Handle dependency cycles
        if !visited.contains(dep) {
            find_deps_dfs(dep, repo, deps, visited)?;
        }
    }

    deps.push(package.to_string());

    Ok(())
}
