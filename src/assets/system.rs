use bevy::{
    asset::{io::Reader, ron, AssetLoader, AsyncReadExt, LoadContext},
    prelude::*,
    utils::ConditionalSendFuture,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct System {
    pub folder: String,
    pub display_name: String,
    pub gravitational_const: f32,
}

#[derive(Default)]
pub struct SystemLoader;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum SystemLoaderError {
    /// An [IO](std::io) Error
    #[error("Could not load asset: {0}")]
    Io(#[from] std::io::Error),
    /// A [RON](ron) Error
    #[error("Could not parse RON: {0}")]
    RonSpannedError(#[from] ron::error::SpannedError),
}

impl AssetLoader for SystemLoader {
    type Asset = System;

    type Settings = ();

    type Error = SystemLoaderError;

    fn load<'a>(
        &'a self,
        reader: &'a mut Reader,
        _settings: &'a (),
        _load_context: &'a mut LoadContext,
    ) -> impl ConditionalSendFuture<
        Output = Result<<Self as AssetLoader>::Asset, <Self as AssetLoader>::Error>,
    > {
        Box::pin(async move {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;
            let asset = ron::de::from_bytes::<System>(&bytes)?;
            Ok(asset)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["system.ron"]
    }
}
