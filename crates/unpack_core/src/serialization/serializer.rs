//! Rust-native implementation of webpack's reusable Serializer responsibility.

use std::{any::Any, collections::HashMap, fmt, io, sync::Arc};

pub(crate) const MAX_SERIALIZED_ITEM_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StableTypeId(pub(crate) [u8; 16]);

impl StableTypeId {
    pub(crate) const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StableCodecId(pub(crate) [u8; 16]);

impl StableCodecId {
    pub(crate) const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

pub(crate) trait SerializableItem: Clone + Send + Sync + 'static {
    const TYPE_ID: StableTypeId;
}

pub(crate) trait ItemCodec<T: SerializableItem>: fmt::Debug + Send + Sync + 'static {
    fn codec_id(&self) -> StableCodecId;
    fn encode(&self, value: &T) -> io::Result<Vec<u8>>;
    fn decode(&self, bytes: &[u8]) -> Option<T>;
}

trait ErasedItemCodec: fmt::Debug + Send + Sync {
    fn codec_id(&self) -> StableCodecId;
    fn encode(&self, value: &dyn Any) -> io::Result<Vec<u8>>;
    fn decode(&self, bytes: &[u8]) -> Option<Box<dyn Any + Send + Sync>>;
}

struct CodecAdapter<T, C> {
    codec: C,
    marker: std::marker::PhantomData<fn() -> T>,
}

impl<T, C: fmt::Debug> fmt::Debug for CodecAdapter<T, C> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodecAdapter")
            .field("codec", &self.codec)
            .finish_non_exhaustive()
    }
}

impl<T, C> ErasedItemCodec for CodecAdapter<T, C>
where
    T: SerializableItem,
    C: ItemCodec<T>,
{
    fn codec_id(&self) -> StableCodecId {
        self.codec.codec_id()
    }

    fn encode(&self, value: &dyn Any) -> io::Result<Vec<u8>> {
        let value = value.downcast_ref::<T>().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "serialized item type mismatch")
        })?;
        self.codec.encode(value)
    }

    fn decode(&self, bytes: &[u8]) -> Option<Box<dyn Any + Send + Sync>> {
        self.codec
            .decode(bytes)
            .map(|value| Box::new(value) as Box<dyn Any + Send + Sync>)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Serializer {
    codecs: HashMap<StableTypeId, Arc<dyn ErasedItemCodec>>,
}

impl Serializer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[allow(dead_code)]
    pub(crate) fn with_codec<T, C>(mut self, codec: C) -> Self
    where
        T: SerializableItem,
        C: ItemCodec<T>,
    {
        self.register::<T, C>(codec);
        self
    }

    pub(crate) fn register<T, C>(&mut self, codec: C)
    where
        T: SerializableItem,
        C: ItemCodec<T>,
    {
        self.codecs.insert(
            T::TYPE_ID,
            Arc::new(CodecAdapter::<T, C> {
                codec,
                marker: std::marker::PhantomData,
            }),
        );
    }

    pub(crate) fn encode<T: SerializableItem>(
        &self,
        value: &T,
    ) -> io::Result<(StableCodecId, Vec<u8>)> {
        let codec = self.codecs.get(&T::TYPE_ID).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "serializer codec is not registered",
            )
        })?;
        let payload = codec.encode(value)?;
        if payload.len() > MAX_SERIALIZED_ITEM_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "serialized item exceeds the configured bound",
            ));
        }
        Ok((codec.codec_id(), payload))
    }

    pub(crate) fn decode<T: SerializableItem>(
        &self,
        type_id: StableTypeId,
        codec_id: StableCodecId,
        bytes: &[u8],
    ) -> Option<T> {
        if type_id != T::TYPE_ID {
            return None;
        }
        let codec = self.codecs.get(&type_id)?;
        if codec.codec_id() != codec_id {
            return None;
        }
        Some(*codec.decode(bytes)?.downcast::<T>().ok()?)
    }

    pub(crate) fn matches_codec(&self, type_id: StableTypeId, codec_id: StableCodecId) -> bool {
        self.codecs
            .get(&type_id)
            .is_some_and(|codec| codec.codec_id() == codec_id)
    }
}
