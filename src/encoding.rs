use std::convert::{TryInto};

type Padding = u32;
pub const PADDING: usize = std::mem::size_of::<Padding>();

/// Encode a message, returning the bytes that must be sent before the message.
pub fn encode_size(message: &[u8]) -> [u8; PADDING] {
    (message.len() as Padding).to_le_bytes()
}

/// Decodes an encoded value in a buffer.
/// The function returns the message size or none if the buffer is less than [`PADDING`].
pub fn decode_size(data: &[u8]) -> Option<usize> {
    data[..PADDING].try_into().map(|data| Padding::from_le_bytes(data) as usize).ok()
}

/// Used to decoded one message from several/partial data chunks
pub struct Decoder {
    processed: Vec<u8>,
    expected_size: Option<usize>,
}

impl Decoder {
    /// Creates a new decoder.
    /// It will only reserve memory in cases decoding needs to keep data among messages.
    pub fn new() -> Decoder {
        Decoder { processed: Vec::new(), expected_size: None }
    }

    fn decoded_len(&self) -> usize {
        self.processed.len() - PADDING
    }

    /// Tries to decode the data without reserve any memory.
    /// Directly from the input data buffer.
    /// The function returns in fist place the decoded data
    /// or `None` if more data is necessary to decode it,
    /// and in second place the reminder data.
    fn try_decode_from_data<'a>(data: &'a[u8]) -> (Option<&'a[u8]>, &'a[u8]) {
        if data.len() >= PADDING {
            let expected_size = decode_size(&data).unwrap();
            if data[PADDING..].len() >= expected_size {
                return (
                    Some(&data[PADDING..PADDING + expected_size]),
                    &data[PADDING + expected_size..],
                )
            }
        }
        (None, data)
    }

    /// The function returns in fist place the decoded data
    /// or `None` if more data is necessary to decode it,
    /// and in second place the reminder data.
    fn try_decode_from_decoder<'a>(&mut self, data: &'a [u8]) -> (Option<&[u8]>, &'a [u8]) {
        // There is decoded data into the decoder with known size
        let next_data = if let Some(expected_size) = self.expected_size {
            let pos = std::cmp::min(expected_size - self.decoded_len(), data.len());
            self.processed.extend_from_slice(&data[..pos]);
            &data[pos..]
        }
        // The data into the decoder and the input data is enough to known the size.
        else if self.processed.len() + data.len() >= PADDING {
            // Deserializing the decoded data size
            let size_pos = std::cmp::min(PADDING - self.processed.len(), PADDING);
            self.processed.extend_from_slice(&data[..size_pos]);
            let expected_size = decode_size(&self.processed).unwrap();
            self.expected_size = Some(expected_size);

            let data = &data[size_pos..];
            if data.len() < expected_size {
                self.processed.extend_from_slice(data);
                &data[data.len()..]
            }
            else {
                self.processed.extend_from_slice(&data[..expected_size]);
                &data[expected_size..]
            }
        }
        // No decoded data into decoder. Not enough data to know about size.
        else {
            self.processed.extend_from_slice(data);
            &data[data.len()..]
        };

        if let Some(expected_size) = self.expected_size {
            if self.decoded_len() == expected_size {
                return (Some(&self.processed[PADDING..]), next_data)
            }
        }

        (None, next_data)
    }

    fn decode_from_empty<'a>(&mut self, data: &'a[u8], mut decoded_callback: impl FnMut(&[u8])) {
        let mut next_data = data;
        loop {
            match Self::try_decode_from_data(next_data) {
                (Some(processed), reminder) =>  {
                    decoded_callback(processed);
                    if reminder.is_empty() {
                        break
                    }
                    next_data = reminder;
                }
                (None, reminder) => {
                    //self.processed.extend_from_slice(reminder);
                    self.try_decode_from_decoder(reminder);
                    break
                }
            };
        }
    }

    pub fn decode(&mut self, data: &[u8], mut decoded_callback: impl FnMut(&[u8])) {
        if self.processed.len() == 0 {
            self.decode_from_empty(data, decoded_callback);
        }
        else { //There was already data in the Decoder
            let (decoded_data, reminder) = self.try_decode_from_decoder(data);
            if let Some(decoded_data) = decoded_data {
                decoded_callback(decoded_data);
                self.processed.clear();
                self.expected_size = None;
                self.decode_from_empty(reminder, decoded_callback);
            }
        }
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
        buffer.extend_from_slice(&encode_size(&MESSAGE));
        buffer.extend_from_slice(&MESSAGE);
    }

    #[test]
    fn encode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        assert_eq!(buffer.len(), ENCODED_MESSAGE_SIZE);
        let expected_size = decode_size(&buffer).unwrap();
        assert_eq!(expected_size, MESSAGE_SIZE);
        assert_eq!(&buffer[PADDING..], &MESSAGE);
    }

    #[test]
    // [ data  ]
    // [message]
    fn fast_decode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        let (decoded_data, next_data) = Decoder::try_decode_from_data(&buffer);
        assert_eq!(next_data.len(), 0);
        assert_eq!(decoded_data.unwrap(), &MESSAGE);
    }

    #[test]
    // [ data  ]
    // [message]
    fn decode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer);

        let mut decoder = Decoder::new();
        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&buffer);
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
            if let (Some(decoded_data), reminder_data) = Decoder::try_decode_from_data(next_data) {
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

        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&first);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);

        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&second);
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

        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&first);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);

        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&second);
        assert_eq!(next_data.len(), ENCODED_MESSAGE_SIZE);
        assert_eq!(decoded_data.unwrap(), &MESSAGE);

        let mut decoder = Decoder::new();
        let (decoded_data, next_data) = decoder.try_decode_from_decoder(next_data);
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
        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&buffer);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);
        buffer.clear();

        buffer.push('a' as u8);
        buffer.push('a' as u8);
        buffer.push('\n' as u8);
        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&buffer);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);
        buffer.clear();

        buffer.push('a' as u8);
        buffer.push('a' as u8);
        buffer.push('a' as u8);
        buffer.push('\n' as u8);
        let (decoded_data, next_data) = decoder.try_decode_from_decoder(&buffer);
        assert!(decoded_data.is_none());
        assert_eq!(next_data.len(), 0);
    }

    //TODO: test DecodingPool
}
