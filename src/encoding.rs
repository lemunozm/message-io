use std::collections::{HashMap, hash_map::Entry};

pub const PADDING: usize = 2;

pub fn encode(data: &mut [u8]) {
    assert!(data.len() >= PADDING, "It is necessary to start serializing after the 'PADDING'");
    let data_size: u16 = (data.len() - PADDING) as u16;
    bincode::serialize_into(data, &data_size).unwrap();
}

pub struct Decoder {
    decoded_data: Vec<u8>,
    expected_size: Option<usize>,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            decoded_data: Vec::new(),
            expected_size: None,
        }
    }

    pub fn try_fast_decode<'a>(data: &'a[u8]) -> Option<(&'a[u8], &'a[u8])> {
        if data.len() >= PADDING {
            let expected_size = bincode::deserialize::<u16>(data).unwrap() as usize;
            if data[PADDING..].len() >= expected_size {
                return Some((
                    &data[PADDING .. PADDING + expected_size], // decoded data
                    &data[PADDING + expected_size ..] // next undecoded data
                ))
            }
        }
        None
    }

    pub fn decode<'a>(&mut self, data: &'a[u8]) -> (Option<&[u8]>, &'a[u8]) {
        let next_data =
        if let Some(expected_size) = self.expected_size {
            let pos = std::cmp::min(expected_size, data.len());
            self.decoded_data.extend_from_slice(&data[..pos]);
            &data[pos..]
        }
        else {
            if data.len() >= PADDING {
                let size_pos = std::cmp::min(PADDING - self.decoded_data.len(), PADDING);
                self.decoded_data.extend_from_slice(&data[..size_pos]);
                let expected_size = bincode::deserialize::<u16>(&self.decoded_data).unwrap() as usize;
                self.expected_size = Some(expected_size);

                let data = &data[size_pos..];
                if data.len() < expected_size {
                    self.decoded_data.extend_from_slice(data);
                    &data[data.len()..]
                }
                else {
                    self.decoded_data.extend_from_slice(&data[..expected_size]);
                    &data[expected_size..]
                }
            }
            else {
                self.decoded_data.extend_from_slice(data);
                &data[data.len()..]
            }
        };

        if let Some(expected_size) = self.expected_size {
            if self.decoded_data.len() == expected_size + PADDING {
                return (Some(&self.decoded_data[PADDING..]), next_data)
            }
        }

        (None, next_data)
    }
}

pub struct DecodingPool<E> {
    decoders: HashMap<E, Decoder>,
}

impl<E> DecodingPool<E>
where E: std::hash::Hash + Eq
{
    pub fn new() -> DecodingPool<E> {
        DecodingPool { decoders: HashMap::new() }
    }

    pub fn decode_from<C: FnMut(&[u8])>(&mut self, data: &[u8], identifier: E, mut decode_callback: C) {
        match self.decoders.entry(identifier) {
            Entry::Vacant(entry) => {
                if let Some(decoder) = Self::fast_decode(data, decode_callback) {
                    entry.insert(decoder);
                }
            }
            Entry::Occupied(mut entry) => {
                let (decoded_data, next_data) = entry.get_mut().decode(data);
                if let Some(decoded_data) = decoded_data {
                    decode_callback(decoded_data);
                    match Self::fast_decode(next_data, decode_callback) {
                        Some(decoder) => entry.insert(decoder),
                        None => entry.remove(),
                    };
                }
            },
        }
    }

    fn fast_decode<C: FnMut(&[u8])>(data: &[u8], mut decode_callback: C) -> Option<Decoder> {
        loop {
            if let Some ((decoded_data, next_data)) = Decoder::try_fast_decode(data) {
                decode_callback(decoded_data);
                if next_data.len() == 0 {
                    return None
                }
            }
            else {
                let mut decoder = Decoder::new();
                decoder.decode(data); // It will not be ready with the reminder data. We safe it and wait the next data.
                return Some(decoder)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_SIZE: usize = 20;

    fn encode_message() -> (Vec<u8>, Vec<u8>) {
        let message = vec![5; MESSAGE_SIZE];
        let mut data = vec![0; PADDING];
        data.extend_from_slice(&message);
        super::encode(&mut data);
        (message, data)
    }

    #[test]
    fn encode() {
        let (message, encoded) = encode_message();

        assert_eq!(encoded.len(), PADDING + MESSAGE_SIZE);
        let expected_size = bincode::deserialize::<u16>(&encoded).unwrap() as usize;
        assert_eq!(expected_size, MESSAGE_SIZE);
        assert_eq!(&encoded[PADDING..], &message[..]);
    }

    #[test]
    fn fast_decode_exact() {
        let (message, encoded) = encode_message();

        let (decoded_data, next_data) = Decoder::try_fast_decode(&encoded[..]).unwrap();
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data, &message[..]);
    }

    #[test]
    fn decode_exact() {
        let (message, encoded) = encode_message();

        let mut decoder = Decoder::new();
        let (decoded_data, next_data) = decoder.decode(&encoded[..]);
        let decoded_data = decoded_data.unwrap();
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data, &message[..]);
    }
}
