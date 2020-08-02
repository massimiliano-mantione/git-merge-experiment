use git2::{BranchType, Oid, Repository};
use std::fs::{create_dir_all, remove_dir_all};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Git(#[from] git2::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

struct RepoRecipe {
    name: String,
    setup: Box<dyn Fn(&RepoRecipe) -> Result<(), Error>>,
}

impl RepoRecipe {
    pub fn new(name: &str, setup: &'static impl Fn(&RepoRecipe) -> Result<(), Error>) -> Self {
        Self {
            name: name.to_owned(),
            setup: Box::new(setup),
        }
    }
}

impl RepoRecipe {
    pub fn name(&self) -> &str {
        &self.name
    }

    fn create(&self) -> Result<(), Error> {
        self.destroy().ok();
        create_dir_all(self.path())?;
        Repository::init(self.path())?;
        (&self.setup)(&self)?;
        Ok(())
    }

    fn destroy(&self) -> Result<(), Error> {
        remove_dir_all(self.path())?;
        Ok(())
    }

    fn repo(&self) -> Result<Repository, Error> {
        Ok(Repository::open(self.path())?)
    }

    fn path(&self) -> String {
        format!("repos/{}", self.name())
    }

    fn tree(&self, entry: &str, data: &str) -> Result<Oid, Error> {
        let repo = self.repo()?;
        let mut tree = repo.treebuilder(None)?;
        tree.insert(entry.to_string(), repo.blob(data.as_bytes())?, 0o100644)?;
        Ok(tree.write()?)
    }

    fn simple_tree(&self) -> Result<Oid, Error> {
        self.tree("data.txt", "text")
    }

    fn commit(&self, branch: &str, message: &str, merges: &[&str]) -> Result<Oid, Error> {
        let repo = self.repo()?;
        let git_sig = repo.signature()?;
        let tree = repo.find_tree(self.simple_tree()?)?;
        let mut parent_branches = Vec::new();
        repo.find_branch(branch, BranchType::Local)
            .map(|branch| parent_branches.push(branch))
            .ok();
        for parent in merges.iter() {
            parent_branches.push(repo.find_branch(parent, BranchType::Local)?);
        }
        let mut parent_commits = Vec::new();
        for branch in parent_branches {
            let target_oid = branch.get().target().unwrap();
            let target_commit = repo.find_commit(target_oid)?;
            parent_commits.push(target_commit);
        }
        let parent_commits_refs: Vec<_> = parent_commits.iter().map(|c| c).collect();
        let commit_oid = repo.commit(
            None,
            &git_sig,
            &git_sig,
            message,
            &tree,
            &parent_commits_refs,
        )?;
        let commit = repo.find_commit(commit_oid)?;
        repo.branch(branch, &commit, true)?;
        Ok(commit_oid)
    }
}

fn main() -> Result<(), Error> {
    RepoRecipe::new("long-diamond", &|repo| {
        repo.commit("bottom", "bottom", &[])?;
        repo.commit("a", "a1", &["bottom"])?;
        repo.commit("a", "a2", &[])?;
        repo.commit("a", "a3", &[])?;
        repo.commit("b", "b1", &["bottom"])?;
        repo.commit("b", "b2", &[])?;
        repo.commit("b", "b3", &[])?;
        repo.commit("top", "top", &["a", "b"])?;
        Ok(())
    })
    .create()?;

    RepoRecipe::new("id-definition", &|repo| {
        // Id document 1 (origin: dev1, delegations: [k1, k2])
        repo.commit("dev1", "doc1", &[])?;
        // Id document 1 signed by k2
        repo.commit("dev2", "doc1-k2", &["dev1"])?;
        // Id document 1 signed by k1
        repo.commit("dev1", "doc1-k1", &[])?;
        // Id attestation 1 (refers to signatures by [k1, k2])
        repo.commit("dev1", "id1", &["dev2"])?;

        // Id document 2 (origin: dev1, delegations: [k1, k2, k3])
        repo.commit("dev1", "doc2", &[])?;
        // Id document 2 signed by k3
        repo.commit("dev3", "doc2-k3", &["dev1"])?;
        // Id document 2 signed by k2
        repo.commit("dev2", "doc2-k2", &["dev1"])?;
        // Id document 2 signed by k1
        repo.commit("dev1", "doc2-k1", &[])?;
        // Id attestation 2 (refers to signatures by [k1, k2, k3])
        repo.commit("dev1", "id2", &["dev2", "dev3"])?;

        // Id document 3 (origin: dev3, delegations: [k2, k3])
        repo.commit("dev3", "doc3", &[])?;
        // Id document 3 signed by k2
        repo.commit("dev2", "doc3-k2", &["dev3"])?;
        // Id document 3 signed by k3
        repo.commit("dev3", "doc3-k3", &[])?;
        // Id attestation 3 (refers to signatures by [k2, k3])
        repo.commit("dev1", "id3", &["dev2"])?;

        repo.commit("top", "top", &["dev1", "dev2", "dev3"])?;
        Ok(())
    })
    .create()?;

    Ok(())
}
