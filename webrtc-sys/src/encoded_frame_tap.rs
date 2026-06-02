// Copyright 2026 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use crate::impl_thread_safety;

#[cxx::bridge(namespace = "livekit_ffi")]
pub mod ffi {
    extern "C++" {
        include!("livekit/encoded_frame_tap.h");

        type NativeEncodedFrameSink;
    }

    unsafe extern "C++" {
        fn new_native_encoded_frame_sink(
            observer: Box<EncodedVideoSinkWrapper>,
        ) -> SharedPtr<NativeEncodedFrameSink>;
    }

    extern "Rust" {
        type EncodedVideoSinkWrapper;

        fn on_encoded_frame(
            self: &EncodedVideoSinkWrapper,
            data_ptr: u64,
            size: u32,
            codec: i32,
            is_key_frame: bool,
            width: u32,
            height: u32,
            timestamp_us: i64,
            rtp_timestamp: u32,
            rotation: i32,
            qp: i32,
            has_qp: bool,
        );
    }
}

impl_thread_safety!(ffi::NativeEncodedFrameSink, Send + Sync);

pub trait EncodedVideoSink: Send {
    fn on_encoded_frame(
        &self,
        data_ptr: u64,
        size: u32,
        codec: i32,
        is_key_frame: bool,
        width: u32,
        height: u32,
        timestamp_us: i64,
        rtp_timestamp: u32,
        rotation: i32,
        qp: i32,
        has_qp: bool,
    );
}

pub struct EncodedVideoSinkWrapper {
    observer: Arc<dyn EncodedVideoSink>,
}

impl EncodedVideoSinkWrapper {
    pub fn new(observer: Arc<dyn EncodedVideoSink>) -> Self {
        Self { observer }
    }

    fn on_encoded_frame(
        &self,
        data_ptr: u64,
        size: u32,
        codec: i32,
        is_key_frame: bool,
        width: u32,
        height: u32,
        timestamp_us: i64,
        rtp_timestamp: u32,
        rotation: i32,
        qp: i32,
        has_qp: bool,
    ) {
        self.observer.on_encoded_frame(
            data_ptr, size, codec, is_key_frame, width, height,
            timestamp_us, rtp_timestamp, rotation, qp, has_qp,
        );
    }
}
