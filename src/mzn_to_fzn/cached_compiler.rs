use crate::args::Args;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use super::Conversion;
use super::compilation;
use super::convert_mzn;

pub struct CachedCompiler {
    args: Args,
    cache: RwLock<HashMap<String, Arc<Conversion>>>,
}


impl CachedCompiler {
    pub fn new(args: Args) -> Self {
        Self {
            args,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub async fn compile(
        &self,
        solver_name: &str,
        cancellation_token: CancellationToken,
    ) -> compilation::Result<Arc<Conversion>> {
        {
            let cache = self.cache.read().await;
            if let Some(conversion) = cache.get(solver_name) {
                return Result::Ok(conversion.clone());
            }
        }

        let conversion = Arc::new(convert_mzn(&self.args, solver_name, cancellation_token).await?);
        let mut cache = self.cache.write().await;
        cache.insert(solver_name.to_owned(), conversion.clone());
        Ok(conversion)
    }
}
