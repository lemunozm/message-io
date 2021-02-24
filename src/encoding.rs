use std::convert::{TryInto};

pub type Padding = u32;
pub const PADDING: usize = std::mem::size_of::<Padding>();

/// Encode a message, returning the bytes that must be sent before the message.
pub fn encode_size(message: &[u8]) -> [u8; PADDING] {
    (message.len() as Padding).to_le_bytes()
}

/// Decodes an encoded value in a buffer.
/// The function returns the message size or none if the buffer is less than [`PADDING`].
pub fn decode_size(data: &[u8]) -> Option<usize> {
    if data.len() < PADDING {
        return None
    }
    data[..PADDING].try_into().map(|encoded| Padding::from_le_bytes(encoded) as usize).ok()
}

/// Used to decoded one message from several/partial data chunks
pub struct Decoder {
    stored: Vec<u8>,
}

impl Default for Decoder {
    /// Creates a new decoder.
    /// It will only reserve memory in cases where decoding needs to keep data among messages.
    fn default() -> Decoder {
        Decoder { stored: Vec::new() }
    }
}

impl Decoder {
    fn try_decode(&mut self, data: &[u8], mut decoded_callback: impl FnMut(&[u8])) {
        let mut next_data = data;
        loop {
            if let Some(expected_size) = decode_size(&next_data) {
                let remaining = &next_data[PADDING..];
                if remaining.len() >= expected_size {
                    let (decoded, not_decoded) = remaining.split_at(expected_size);
                    decoded_callback(decoded);
                    if !not_decoded.is_empty() {
                        next_data = not_decoded;
                        continue
                    }
                    else {
                        break
                    }
                }
            }
            self.stored.extend_from_slice(next_data);
            break
        }
    }

    fn store_and_decoded_data<'a>(&mut self, data: &'a [u8]) -> Option<(&[u8], &'a [u8])> {
        // Process frame header
        let expected_size = match decode_size(&self.stored) {
            Some(size) => size,
            None => {
                let remaining = PADDING - self.stored.len();
                if data.len() > remaining {
                    // Now, we can now the size
                    self.stored.extend_from_slice(&data[..remaining]);
                    decode_size(&self.stored).unwrap()
                }
                else {
                    // We need more data to know the size
                    self.stored.extend_from_slice(data);
                    return None
                }
            }
        };

        // At this point we know at least the expected size of the frame.
        let remaining = expected_size - (self.stored.len() - PADDING);
        if data.len() < remaining {
            // We need more data to decoder
            self.stored.extend_from_slice(data);
            None
        }
        else {
            // We can complete a message here
            let (to_store, remaining) = data.split_at(remaining);
            self.stored.extend_from_slice(to_store);
            Some((&self.stored[PADDING..], remaining))
        }
    }

    /// Tries to decode data without reserve any memory, direcly from `data`.
    /// `decoded_callback` will be called for each decoded message.
    /// If `data` is not enough to decoding a message, the data will be stored
    /// until more data is decoded (more successives calls to this function).
    pub fn decode(&mut self, data: &[u8], mut decoded_callback: impl FnMut(&[u8])) {
        if self.stored.is_empty() {
            self.try_decode(data, decoded_callback);
        }
        else {
            //There was already data in the Decoder
            if let Some((decoded_data, remaining)) = self.store_and_decoded_data(data) {
                decoded_callback(decoded_data);
                self.stored.clear();
                self.try_decode(remaining, decoded_callback);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_SIZE: usize = 20; // only works if (X + PADDING ) % 6 == 0
    const ENCODED_MESSAGE_SIZE: usize = PADDING + MESSAGE_SIZE;
    const MESSAGE: [u8; MESSAGE_SIZE] = [42; MESSAGE_SIZE];
    const MESSAGE_A: [u8; MESSAGE_SIZE] = ['A' as u8; MESSAGE_SIZE];
    const MESSAGE_B: [u8; MESSAGE_SIZE] = ['B' as u8; MESSAGE_SIZE];
    const MESSAGE_C: [u8; MESSAGE_SIZE] = ['C' as u8; MESSAGE_SIZE];

    fn encode_message(buffer: &mut Vec<u8>, message: &[u8]) {
        buffer.extend_from_slice(&encode_size(&MESSAGE));
        buffer.extend_from_slice(message);
    }

    #[test]
    fn encode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &MESSAGE);

        assert_eq!(ENCODED_MESSAGE_SIZE, buffer.len());
        let expected_size = decode_size(&buffer).unwrap();
        assert_eq!(MESSAGE_SIZE, expected_size);
        assert_eq!(&MESSAGE, &buffer[PADDING..]);
    }

    #[test]
    // [ data  ]
    // [message]
    fn decode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &MESSAGE);

        let mut decoder = Decoder::default();
        let mut times_called = 0;
        decoder.decode(&buffer, |decoded| {
            times_called += 1;
            assert_eq!(MESSAGE, decoded);
        });

        assert_eq!(1, times_called);
        assert_eq!(0, decoder.stored.len());
    }

    #[test]
    // [          data           ]
    // [message][message][message]
    fn decode_multiple_messages_exact() {
        let mut buffer = Vec::new();

        let messages = [&MESSAGE_A, &MESSAGE_B, &MESSAGE_C];
        encode_message(&mut buffer, messages[0]);
        encode_message(&mut buffer, messages[1]);
        encode_message(&mut buffer, messages[2]);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        decoder.decode(&buffer, |decoded| {
            assert_eq!(messages[times_called], decoded);
            times_called += 1;
        });

        assert_eq!(3, times_called);
        assert_eq!(0, decoder.stored.len());
    }

    #[test]
    // [ data ][ data ]
    // [    message   ]
    fn decode_one_message_in_two_parts() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &MESSAGE);

        const SPLIT: usize = ENCODED_MESSAGE_SIZE / 2;
        let (first, second) = buffer.split_at(SPLIT);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        decoder.decode(&first, |_decoded| {
            // Should not be called
            times_called += 1;
        });

        assert_eq!(0, times_called);
        assert_eq!(SPLIT, decoder.stored.len());

        decoder.decode(&second, |decoded| {
            times_called += 1;
            assert_eq!(MESSAGE, decoded);
        });

        assert_eq!(1, times_called);
        assert_eq!(0, decoder.stored.len());
    }

    #[test]
    // [ data ][        data        ]
    // [   message   ][   message   ]
    fn decode_two_messages_in_two_parts() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &MESSAGE);
        encode_message(&mut buffer, &MESSAGE);

        const SPLIT: usize = ENCODED_MESSAGE_SIZE * 2 / 3;
        let (first, second) = buffer.split_at(SPLIT);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        decoder.decode(&first, |_decoded| {
            // Should not be called
            times_called += 1;
        });

        assert_eq!(0, times_called);
        assert_eq!(SPLIT, decoder.stored.len());

        decoder.decode(&second, |decoded| {
            times_called += 1;
            assert_eq!(MESSAGE, decoded);
        });

        assert_eq!(2, times_called);
        assert_eq!(0, decoder.stored.len());
    }

    #[test]
    // [ 1B ][ 1B ][...][ 1B ]
    // [       message       ]
    fn decode_byte_per_byte() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &MESSAGE);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        for i in 0..buffer.len() {
            decoder.decode(&buffer[i..i + 1], |decoded| {
                assert_eq!(buffer.len() - 1, i);
                times_called += 1;
                assert_eq!(MESSAGE, decoded);
            });

            if i < buffer.len() - 1 {
                assert_eq!(i + 1, decoder.stored.len());
            }
        }

        assert_eq!(0, decoder.stored.len());
        assert_eq!(1, times_called);
    }
}
