use std::collections::{HashMap, hash_map::Entry};

const PADDING: usize = 2;

pub fn encode<C: Fn(&mut Vec<u8>)>(buffer: &mut Vec<u8>, encode_callback: C) {
    let start_point = buffer.len();
    buffer.extend_from_slice(&[0; PADDING]); //Start serializing after PADDING
    let message_point = buffer.len();
    encode_callback(buffer);
    assert!(buffer.len() >= message_point, "Encoding must not decrement the buffer length");
    let data_size: u16 = (buffer.len() - message_point) as u16;
    bincode::serialize_into(&mut buffer[start_point..start_point + PADDING], &data_size).unwrap();
}

pub struct Decoder {
    decoded_data: Vec<u8>,
    expected_size: Option<usize>,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder { decoded_data: Vec::new(), expected_size: None }
    }

    pub fn try_fast_decode<'a>(data: &'a [u8]) -> Option<(&'a [u8], &'a [u8])> {
        if data.len() >= PADDING {
            let expected_size = bincode::deserialize::<u16>(data).unwrap() as usize;
            if data[PADDING..].len() >= expected_size {
                return Some((
                    &data[PADDING..PADDING + expected_size], // decoded data
                    &data[PADDING + expected_size..],        // next undecoded data
                ))
            }
        }
        None
    }

    pub fn decode<'a>(&mut self, data: &'a [u8]) -> (Option<&[u8]>, &'a [u8]) {
        let next_data = if let Some(expected_size) = self.expected_size {
            let pos = std::cmp::min(expected_size, data.len());
            self.decoded_data.extend_from_slice(&data[..pos]);
            &data[pos..]
        }
        else {
            if data.len() >= PADDING {
                let size_pos = std::cmp::min(PADDING - self.decoded_data.len(), PADDING);
                self.decoded_data.extend_from_slice(&data[..size_pos]);
                let expected_size =
                    bincode::deserialize::<u16>(&self.decoded_data).unwrap() as usize;
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

    pub fn decode_from<C: FnMut(&[u8])>(
        &mut self,
        data: &[u8],
        identifier: E,
        mut decode_callback: C,
    )
    {
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
            }
        }
    }

    fn fast_decode<C: FnMut(&[u8])>(data: &[u8], mut decode_callback: C) -> Option<Decoder> {
        let mut next_data = data;
        loop {
            if let Some((decoded_data, reminder_data)) = Decoder::try_fast_decode(next_data) {
                decode_callback(decoded_data);
                if reminder_data.len() == 0 {
                    return None
                }
                next_data = reminder_data;
            }
            else {
                let mut decoder = Decoder::new();
                decoder.decode(next_data); // It will not be ready with the reminder data. We safe it and wait the next data.
                return Some(decoder)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_VALUE: u8 = 5;
    const MESSAGE_SIZE: usize = 20;
    const MESSAGE: [u8; MESSAGE_SIZE] = [MESSAGE_VALUE; MESSAGE_SIZE];

    fn encode_message(buffer: &mut Vec<u8>) {
        super::encode(buffer, |slot| {
            slot.extend_from_slice(&MESSAGE);
        });
    }

    #[test]
    fn encode() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        assert_eq!(buffer.len(), PADDING + MESSAGE_SIZE);
        let expected_size = bincode::deserialize::<u16>(&buffer[0..]).unwrap() as usize;
        assert_eq!(expected_size, MESSAGE_SIZE);
        assert_eq!(&buffer[PADDING..], &MESSAGE);
    }

    #[test]
    fn fast_decode_exact() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        let (decoded_data, next_data) = Decoder::try_fast_decode(&buffer).unwrap();
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data, &MESSAGE);
    }

    #[test]
    fn decode_exact() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        let mut decoder = Decoder::new();
        let (decoded_data, next_data) = decoder.decode(&buffer);
        let decoded_data = decoded_data.unwrap();
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data, &MESSAGE);
    }

    #[test]
    fn fast_decode_exact_multiple() {
        const MESSAGES_NUMBER: usize = 3;

        let mut buffer = Vec::new();
        for _ in 0..MESSAGES_NUMBER {
            encode_message(&mut buffer);
        }

        let mut message_index = 0;
        let mut next_data = &buffer[..];
        loop {
            message_index += 1;
            if let Some((decoded_data, reminder_data)) = Decoder::try_fast_decode(next_data) {
                println!("{}", message_index);
                assert_eq!(
                    reminder_data.len(),
                    (MESSAGE_SIZE + PADDING) * (MESSAGES_NUMBER - message_index)
                );
                assert_eq!(decoded_data, &MESSAGE);
                if reminder_data.len() == 0 {
                    assert_eq!(message_index, MESSAGES_NUMBER);
                    break
                }
                next_data = reminder_data;
            }
        }
    }

    //TODO: test DecodingPool
}
