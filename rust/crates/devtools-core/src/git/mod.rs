//! Git: in-process reads via git2 (same libgit2 the C# build bound, so the
//! semantics carry over exactly) and CLI mutations through the shared process
//! runner, where hooks, gpg signing and credential helpers keep applying.

pub mod cli;
pub mod read;
