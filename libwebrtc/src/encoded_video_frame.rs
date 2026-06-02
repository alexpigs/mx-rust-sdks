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

use crate::video_frame::{FrameMetadata, VideoRotation};

/// Codec types matching the VideoCodec enum in protobuf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VideoCodecType {
    VP8 = 0,
    H264 = 1,
    AV1 = 2,
    VP9 = 3,
    H265 = 4,
}

impl From<i32> for VideoCodecType {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::VP8,
            1 => Self::H264,
            2 => Self::AV1,
            3 => Self::VP9,
            4 => Self::H265,
            _ => Self::H264,
        }
    }
}

/// Encapsulation format of the encoded payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodedPayloadFormat {
    Unknown = 0,
    WebRtc = 1,
    H264AnnexB = 2,
    H264Avcc = 3,
}

impl From<i32> for EncodedPayloadFormat {
    fn from(v: i32) -> Self {
        match v {
            0 => Self::Unknown,
            1 => Self::WebRtc,
            2 => Self::H264AnnexB,
            3 => Self::H264Avcc,
            _ => Self::Unknown,
        }
    }
}

/// A single depacketized encoded video frame.
#[derive(Debug, Clone)]
pub struct EncodedVideoFrame {
    pub data: Vec<u8>,
    pub codec: VideoCodecType,
    pub payload_format: EncodedPayloadFormat,
    pub is_key_frame: bool,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: i64,
    pub rtp_timestamp: u32,
    pub rotation: VideoRotation,
    pub qp: Option<i32>,
    pub metadata: Option<FrameMetadata>,
}

/// Codec configuration delivered when the receiver negotiates parameters.
#[derive(Debug, Clone)]
pub struct EncodedVideoCodecConfig {
    pub codec: VideoCodecType,
    pub profile_level_id: Option<String>,
    pub sps: Option<Vec<u8>>,
    pub pps: Option<Vec<u8>>,
    pub width: u32,
    pub height: u32,
}
