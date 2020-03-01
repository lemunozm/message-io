use std::collections::{HashMap, hash_map::Entry};

type Padding = u32;
pub const PADDING: usize = std::mem::size_of::<Padding>();

/// Prepare the buffer to encode data.
/// The user should copy the data to the callback buffer in order to encode it.
pub fn encode<C: Fn(&mut Vec<u8>)>(buffer: &mut Vec<u8>, encode_callback: C) {
    let start_point = buffer.len();
    buffer.extend_from_slice(&[0; PADDING]); //Start serializing after PADDING
    let message_point = buffer.len();
    encode_callback(buffer);
    assert!(buffer.len() >= message_point, "Encoding must not decrement the buffer length");
    let data_size = (buffer.len() - message_point) as Padding;
    bincode::serialize_into(&mut buffer[start_point..start_point + PADDING], &data_size).unwrap();
}

/// Used to decoded one message from several/partial data chunks
pub struct Decoder {
    decoded_data: Vec<u8>,
    expected_size: Option<usize>,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder { decoded_data: Vec::new(), expected_size: None }
    }

    pub fn decoded_len(&self) -> usize {
        self.decoded_data.len() - PADDING
    }

    /// Tries to decode the data without reserve any memory.
    /// The function returns the decoded data and the remaining data or `None`
    /// if more data is necessary to decode it.
    /// If this function returns None, a call to `decode()` is needed.
    pub fn try_fast_decode(data: &[u8]) -> Option<(&[u8], &[u8])> {
        if data.len() >= PADDING {
            let expected_size = bincode::deserialize::<Padding>(data).unwrap() as usize;
            if data[PADDING..].len() >= expected_size {
                return Some((
                    &data[PADDING..PADDING + expected_size], // decoded data
                    &data[PADDING + expected_size..],        // next undecoded data
                ))
            }
        }
        None
    }

    /// Given data, it tries to decode a message grouping data if necessary.
    /// The function returns a tuple.
    /// In the first place it returns the decoded data or `None`
    /// if it can not be decoded yet because it needs more data.
    /// In second place it returns the remaing data.
    /// If decode returns decoded data, this decoded is no longer usable.
    pub fn decode<'a>(&mut self, data: &'a [u8]) -> (Option<&[u8]>, &'a [u8]) {
        // There is decoded data into the decoder
        let next_data = if let Some(expected_size) = self.expected_size {
            let pos = std::cmp::min(expected_size - self.decoded_len(), data.len());
            self.decoded_data.extend_from_slice(&data[..pos]);
            &data[pos..]
        }
        // No decoded data into decoder.
        else if self.decoded_data.len() + data.len() >= PADDING {
            // Deserializing the decoded data size
            let size_pos = std::cmp::min(PADDING - self.decoded_data.len(), PADDING);
            self.decoded_data.extend_from_slice(&data[..size_pos]);
            let expected_size =
                bincode::deserialize::<Padding>(&self.decoded_data).unwrap() as usize;
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
        // No decoded data into decoder. Not enough data to know about size.
        else {
            self.decoded_data.extend_from_slice(data);
            &data[data.len()..]
        };

        if let Some(expected_size) = self.expected_size {
            if self.decoded_len() == expected_size {
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

    pub fn decode_from<C: FnMut(&[u8])>(&mut self, data: &[u8], id: E, mut decode_callback: C) {
        match self.decoders.entry(id) {
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
                if reminder_data.is_empty() {
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

    pub fn remove_if_exists(&mut self, id: E) {
        self.decoders.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_VALUE: u8 = 5;
    const MESSAGE_SIZE: usize = 20; // only works if (X + PADDING ) % 6 == 0
    const ENCODED_MESSAGE_SIZE: usize = PADDING + MESSAGE_SIZE;
    const MESSAGE: [u8; MESSAGE_SIZE] = [MESSAGE_VALUE; MESSAGE_SIZE];

    fn encode_message(buffer: &mut Vec<u8>) {
        super::encode(buffer, |slot| {
            slot.extend_from_slice(&MESSAGE);
        });
    }

    #[test]
    fn encode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        assert_eq!(buffer.len(), ENCODED_MESSAGE_SIZE);
        let expected_size = bincode::deserialize::<Padding>(&buffer[0..]).unwrap() as usize;
        assert_eq!(expected_size, MESSAGE_SIZE);
        assert_eq!(&buffer[PADDING..], &MESSAGE);
    }

    #[test]
    // [ data  ]
    // [message]
    fn fast_decode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        let (decoded_data, next_data) = Decoder::try_fast_decode(&buffer).unwrap();
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data, &MESSAGE);
    }

    #[test]
    // [ data  ]
    // [message]
    fn decode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        let mut decoder = Decoder::new();
        let (decoded_data, next_data) = decoder.decode(&buffer);
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data.unwrap(), &MESSAGE);
    }

    #[test]
    // [          data           ]
    // [message][message][message]
    fn fast_decode_multiple_messages() {
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
                assert_eq!(
                    reminder_data.len(),
                    (ENCODED_MESSAGE_SIZE) * (MESSAGES_NUMBER - message_index)
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

    #[test]
    // [ data ][ data ]
    // [    message   ]
    fn decode_one_message_in_two_parts() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        let mut decoder = Decoder::new();
        let (first, second) = buffer.split_at(ENCODED_MESSAGE_SIZE / 2);

        let (decoded_data, next_data) = decoder.decode(&first);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);

        let (decoded_data, next_data) = decoder.decode(&second);
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data.unwrap(), &MESSAGE);
    }

    #[test]
    // [ data ][        data        ]
    // [   message   ][   message   ]
    fn decode_two_messages_in_two_parts() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);
        encode_message(&mut buffer);

        let mut decoder = Decoder::new();
        let (first, second) = buffer.split_at(ENCODED_MESSAGE_SIZE * 2 / 3);

        let (decoded_data, next_data) = decoder.decode(&first);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);

        let (decoded_data, next_data) = decoder.decode(&second);
        assert_eq!(next_data.len(), ENCODED_MESSAGE_SIZE);
        assert_eq!(decoded_data.unwrap(), &MESSAGE);

        let mut decoder = Decoder::new();
        let (decoded_data, next_data) = decoder.decode(next_data);
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data.unwrap(), &MESSAGE);
    }

    #[test]
    // Data content:
    // a\n   -> 2 < PADDING
    // aa\n  -> 5 > PADDING
    // aaa\n -> 9 > PADDING
    fn decode_len_less_than_padding() {
        let mut buffer = Vec::new();
        let mut decoder = Decoder::new();

        buffer.push('a' as u8);
        buffer.push('\n' as u8);
        let (decoded_data, next_data) = decoder.decode(&buffer);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);
        buffer.clear();

        buffer.push('a' as u8);
        buffer.push('a' as u8);
        buffer.push('\n' as u8);
        let (decoded_data, next_data) = decoder.decode(&buffer);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);
        buffer.clear();

        buffer.push('a' as u8);
        buffer.push('a' as u8);
        buffer.push('a' as u8);
        buffer.push('\n' as u8);
        let (decoded_data, next_data) = decoder.decode(&buffer);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);
    }

    //TODO: test DecodingPool
}
