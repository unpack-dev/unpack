mod serializer;

pub(crate) use serializer::{
    ItemCodec, MAX_SERIALIZED_ITEM_BYTES, SerializableItem, Serializer, StableCodecId, StableTypeId,
};
