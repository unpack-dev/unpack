// Webpack source: https://github.com/webpack/webpack/blob/da91761ed92c8e133ee321c7db4ad6c4698cae0a/lib/Compilation.js

use std::{fmt::Debug, future::Future, pin::Pin};

use crate::{Compilation, Result};

pub type HookFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// Host callbacks invoked at webpack-compatible compilation boundaries.
pub trait CompilationHooks: Debug + Send + Sync {
    fn compilation<'a>(&'a self, compilation: &'a Compilation) -> HookFuture<'a>;
    fn finish_modules<'a>(&'a self, compilation: &'a mut Compilation) -> HookFuture<'a>;
}
