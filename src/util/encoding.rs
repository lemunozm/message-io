use integer_encoding::VarInt;

/// This is the max required bytes to encode a u64 using the varint encoding scheme.
/// It is size 10=ceil(64/7)
pub const MAX_ENCODED_SIZE: usize = 10;

/// Encode a message, returning the bytes that must be sent before the message.
/// A buffer is used to avoid heap allocation.
pub fn encode_size<'a>(message: &[u8], buf: &'a mut [u8; MAX_ENCODED_SIZE]) -> &'a [u8] {
    let varint_size = message.len().encode_var(buf);
    &buf[..varint_size]
}

/// Decodes an encoded value in a buffer.
/// The function returns the message size and the consumed bytes or none if the buffer is too small.
pub fn decode_size(data: &[u8]) -> Option<(usize, usize)> {
    usize::decode_var(data)
}

/// Used to decoded messages from several/partial data chunks
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
            if let Some((expected_size, used_bytes)) = decode_size(next_data) {
                let remaining = &next_data[used_bytes..];
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
        let ((expected_size, used_bytes), data) = match decode_size(&self.stored) {
            Some(size_info) => (size_info, data),
            None => {
                // we append at most the potential data needed to decode the size
                let max_remaining = (MAX_ENCODED_SIZE - self.stored.len()).min(data.len());
                self.stored.extend_from_slice(&data[..max_remaining]);

                if let Some(x) = decode_size(&self.stored) {
                    // Now we know the size
                    (x, &data[max_remaining..])
                }
                else {
                    // We still don't know the size (data was too small)
                    return None
                }
            }
        };

        // At this point we know at least the expected size of the frame.
        let remaining = expected_size - (self.stored.len() - used_bytes);
        if data.len() < remaining {
            // We need more data to decoder
            self.stored.extend_from_slice(data);
            None
        }
        else {
            // We can complete a message here
            let (to_store, remaining) = data.split_at(remaining);
            self.stored.extend_from_slice(to_store);
            Some((&self.stored[used_bytes..], remaining))
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

    /// Returns the bytes len stored in this decoder.
    /// It can include both, the padding bytes and the data message bytes.
    /// After decoding a message, its bytes are removed from the decoder.
    pub fn stored_size(&self) -> usize {
        self.stored.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MESSAGE_SIZE: usize = 20; // only works if (X + PADDING ) % 6 == 0
    const ENCODED_MESSAGE_SIZE: usize = 1 + MESSAGE_SIZE; // 1 = log_2(20)/7
    const MESSAGE: [u8; MESSAGE_SIZE] = [42; MESSAGE_SIZE];
    const MESSAGE_A: [u8; MESSAGE_SIZE] = ['A' as u8; MESSAGE_SIZE];
    const MESSAGE_B: [u8; MESSAGE_SIZE] = ['B' as u8; MESSAGE_SIZE];
    const MESSAGE_C: [u8; MESSAGE_SIZE] = ['C' as u8; MESSAGE_SIZE];

    fn encode_message(buffer: &mut Vec<u8>, message: &[u8]) {
        let mut buf = [0; MAX_ENCODED_SIZE];
        buffer.extend_from_slice(&*encode_size(message, &mut buf));
        buffer.extend_from_slice(message);
    }

    #[test]
    fn encode_one_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &MESSAGE);

        assert_eq!(ENCODED_MESSAGE_SIZE, buffer.len());
        let (expected_size, used_bytes) = decode_size(&buffer).unwrap();
        assert_eq!(MESSAGE_SIZE, expected_size);
        assert_eq!(used_bytes, 1);
        assert_eq!(&MESSAGE, &buffer[used_bytes..]);
    }

    #[test]
    fn encode_one_big_message() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &vec![0; 1000]);

        assert_eq!(1002, buffer.len());
        let (expected_size, used_bytes) = decode_size(&buffer).unwrap();
        assert_eq!(1000, expected_size);
        assert_eq!(used_bytes, 2);
        assert_eq!(&vec![0; 1000], &buffer[used_bytes..]);
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
    // [          4B         ]
    // [       message       ]
    fn decode_message_no_size() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &[]);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        decoder.decode(&buffer, |_decoded| {
            // Should not be called
            times_called += 1;
        });

        assert_eq!(1, times_called);
        assert_eq!(0, decoder.stored.len());
    }

    #[test]
    // [          5B          ]
    // [        message       ]
    fn decode_message_one_byte() {
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &[0xFF]);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        decoder.decode(&buffer, |decoded| {
            times_called += 1;
            assert_eq!([0xFF], decoded);
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

    #[test]
    // [ 1B ][   remaining   ]
    // [       message       ]
    fn decode_message_after_non_enough_padding() {
        let msg = [0; 1000];
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &msg);

        let (start_1b, remaining) = buffer.split_at(2);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        decoder.decode(&start_1b, |_decoded| {
            // Should not be called
            times_called += 1;
        });

        assert_eq!(0, times_called);
        assert_eq!(2, decoder.stored.len());

        decoder.decode(&remaining, |decoded| {
            times_called += 1;
            assert_eq!(msg, decoded);
        });

        assert_eq!(1, times_called);
        assert_eq!(0, decoder.stored.len());
    }

    #[test]
    // [ 1B ][ 1B ][ remaining   ]
    // [         message         ]
    fn decode_message_var_size_in_two_data() {
        let msg = [0; 1000];
        let mut buffer = Vec::new();
        encode_message(&mut buffer, &msg);

        let (start_1b, remaining) = buffer.split_at(1);

        let mut decoder = Decoder::default();

        let mut times_called = 0;
        decoder.decode(&start_1b, |_decoded| {
            // Should not be called
            times_called += 1;
        });

        assert_eq!(0, times_called);
        assert_eq!(1, decoder.stored.len());

        let (next_1b, remaining) = remaining.split_at(1);

        let mut times_called = 0;
        decoder.decode(&next_1b, |_decoded| {
            // Should not be called
            times_called += 1;
        });

        assert_eq!(0, times_called);
        assert_eq!(2, decoder.stored.len());

        decoder.decode(&remaining, |decoded| {
            times_called += 1;
            assert_eq!(msg, decoded);
        });

        assert_eq!(1, times_called);
        assert_eq!(0, decoder.stored.len());
    }
}
