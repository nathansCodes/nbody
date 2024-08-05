use bevy::{
    asset::{io::Reader, ron, AssetLoader, AsyncReadExt, LoadContext},
    prelude::*,
    utils::ConditionalSendFuture,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Asset, TypePath, Debug, Deserialize)]
pub struct Body {
    pub initial_pos: Vec2,
    /// in m/s
    pub velocity: Vec2,
    /// in kg
    pub mass: f32,
    pub radius: f32,
    pub color: Color,
    pub name: String,
}

#[derive(Default)]
pub struct BodyLoader;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum BodyLoaderError {
    /// An [IO](std::io) Error
    #[error("Could not load asset: {0}")]
    Io(#[from] std::io::Error),
    /// A [RON](ron) Error
    #[error("Could not parse RON: {0}")]
    RonSpannedError(#[from] ron::error::SpannedError),
}

impl AssetLoader for BodyLoader {
    type Asset = Body;

    type Settings = ();

    type Error = BodyLoaderError;

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
            let asset = ron::de::from_bytes::<Body>(&bytes)?;
            Ok(asset)
        })
    }

    fn extensions(&self) -> &[&str] {
        &["body.ron"]
    }
}
