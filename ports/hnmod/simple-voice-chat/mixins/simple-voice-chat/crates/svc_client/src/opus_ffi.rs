//! Hand-written FFI over the vendored libopus 1.4 C library.
//! libopus is the Xiph.Org reference Opus codec, BSD three-clause licensed;
//! this wrapper is clean-room and only exposes what the audio module needs.

use std::os::raw::c_int;

const OPUS_OK: c_int = 0;
const OPUS_SET_BITRATE: c_int = 4002;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub c_int);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "opus error {}", self.0)
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Mono,
    #[allow(dead_code)]
    Stereo,
}

impl From<Channels> for c_int {
    fn from(c: Channels) -> c_int {
        match c {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Application {
    Voip,
    #[allow(dead_code)]
    Audio,
    #[allow(dead_code)]
    LowDelay,
}

impl From<Application> for c_int {
    fn from(a: Application) -> c_int {
        match a {
            Application::Voip => 2049,
            Application::Audio => 2050,
            Application::LowDelay => 2051,
        }
    }
}

#[repr(C)]
pub struct OpusEncoder {
    _opaque: [u8; 0],
}

#[repr(C)]
pub struct OpusDecoder {
    _opaque: [u8; 0],
}

extern "C" {
    fn opus_encoder_create(
        fs: c_int,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut OpusEncoder;
    fn opus_encoder_destroy(st: *mut OpusEncoder);
    fn opus_encode(
        st: *mut OpusEncoder,
        pcm: *const i16,
        frame_size: c_int,
        data: *mut u8,
        max_data_bytes: c_int,
    ) -> c_int;
    fn opus_encoder_ctl(st: *mut OpusEncoder, request: c_int, ...) -> c_int;
    fn opus_decoder_create(fs: c_int, channels: c_int, error: *mut c_int) -> *mut OpusDecoder;
    fn opus_decoder_destroy(st: *mut OpusDecoder);
    fn opus_decode(
        st: *mut OpusDecoder,
        data: *const u8,
        len: c_int,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;
}

pub struct Encoder {
    st: *mut OpusEncoder,
    frame_size: usize,
    channels: usize,
}

unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new(
        sample_rate: u32,
        channels: Channels,
        application: Application,
    ) -> Result<Self, Error> {
        let mut err: c_int = 0;
        let st = unsafe {
            opus_encoder_create(
                sample_rate as c_int,
                channels.into(),
                application.into(),
                &mut err,
            )
        };
        if err != OPUS_OK || st.is_null() {
            return Err(Error(err));
        }
        Ok(Self {
            st,
            frame_size: sample_rate as usize / 50,
            channels: c_int::from(channels) as usize,
        })
    }

    pub fn set_bitrate(&mut self, bitrate: u32) -> Result<(), Error> {
        let r = unsafe { opus_encoder_ctl(self.st, OPUS_SET_BITRATE, bitrate as c_int) };
        if r == OPUS_OK {
            Ok(())
        } else {
            Err(Error(r))
        }
    }

    /// Encode one 20 ms frame into `dst`; returns the packet length in bytes.
    pub fn encode(&mut self, input: &[i16], dst: &mut [u8]) -> Result<usize, Error> {
        debug_assert_eq!(input.len(), self.frame_size * self.channels);
        let n = unsafe {
            opus_encode(
                self.st,
                input.as_ptr(),
                self.frame_size as c_int,
                dst.as_mut_ptr(),
                dst.len() as c_int,
            )
        };
        if n < 0 {
            return Err(Error(n));
        }
        Ok(n as usize)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { opus_encoder_destroy(self.st) }
    }
}

pub struct Decoder {
    st: *mut OpusDecoder,
    channels: usize,
}

unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new(sample_rate: u32, channels: Channels) -> Result<Self, Error> {
        let mut err: c_int = 0;
        let st = unsafe { opus_decoder_create(sample_rate as c_int, channels.into(), &mut err) };
        if err != OPUS_OK || st.is_null() {
            return Err(Error(err));
        }
        Ok(Self {
            st,
            channels: c_int::from(channels) as usize,
        })
    }

    /// Decode one packet into `output`; an empty input decodes loss
    /// concealment (optionally forward error correction).
    pub fn decode(
        &mut self,
        input: Option<&[u8]>,
        output: &mut [i16],
        fec: bool,
    ) -> Result<usize, Error> {
        let (ptr, len) = match input {
            Some(p) if !p.is_empty() => (p.as_ptr(), p.len() as c_int),
            _ => (std::ptr::null(), 0),
        };
        let frame = output.len() / self.channels;
        let n = unsafe {
            opus_decode(
                self.st,
                ptr,
                len,
                output.as_mut_ptr(),
                frame as c_int,
                fec as c_int,
            )
        };
        if n < 0 {
            return Err(Error(n));
        }
        Ok(n as usize)
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { opus_decoder_destroy(self.st) }
    }
}
