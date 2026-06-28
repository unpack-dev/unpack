use std::path::{Path, PathBuf};

use crate::{Dependency, ModuleIdentity, Result, UnpackResolver};

#[derive(Debug, Clone)]
pub struct NormalModuleFactory {
    resolver: UnpackResolver,
}

impl NormalModuleFactory {
    pub fn new(resolver: UnpackResolver) -> Self {
        Self { resolver }
    }

    pub async fn factorize(
        &self,
        context: &Path,
        dependency: &Dependency,
    ) -> Result<FactorizedModule> {
        let request = dependency
            .request()
            .expect("module dependency should have a request");
        let resolved = self.resolver.resolve(context, request).await?;
        let identity = ModuleIdentity::from(resolved);
        let resource = identity.resource.clone();
        Ok(FactorizedModule { identity, resource })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactorizedModule {
    pub identity: ModuleIdentity,
    pub resource: PathBuf,
}
