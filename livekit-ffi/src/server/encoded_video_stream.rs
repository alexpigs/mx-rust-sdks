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

use futures_util::StreamExt;
use livekit::{
    prelude::Track,
    webrtc::{
        prelude::*,
        video_stream::encoded::NativeEncodedVideoStream,
    },
};
use tokio::sync::oneshot;

use super::{room::FfiTrack, FfiHandle};
use crate::{proto, server, FfiError, FfiHandleId, FfiResult};

pub struct FfiEncodedVideoStream {
    pub handle_id: FfiHandleId,
    #[allow(dead_code)]
    self_dropped_tx: oneshot::Sender<()>,
}

impl FfiHandle for FfiEncodedVideoStream {}

pub struct EncodedVideoBufferHandle {
    pub data: Vec<u8>,
}

impl FfiHandle for EncodedVideoBufferHandle {}

impl FfiEncodedVideoStream {
    pub fn from_track(
        server: &'static server::FfiServer,
        request: proto::NewEncodedVideoStreamRequest,
    ) -> FfiResult<proto::OwnedEncodedVideoStream> {
        let ffi_track = server
            .retrieve_handle::<FfiTrack>(request.track_handle)?
            .clone();

        let Track::RemoteVideo(remote_track) = &ffi_track.track else {
            return Err(FfiError::InvalidRequest(
                "encoded video stream requires a remote video track".into(),
            ));
        };

        let transceiver = remote_track.transceiver().ok_or_else(|| {
            FfiError::InvalidRequest("remote video track has no transceiver".into())
        })?;

        let rtc_receiver = transceiver.receiver();
        let capacity = request.queue_size_frames.map(|c| c as usize);
        let drop_after_tap = request.drop_after_tap.unwrap_or(false);

        // Query codec BEFORE moving rtc_receiver into the spawned task
        let params = rtc_receiver.parameters();
        let codec = params
            .codecs
            .first()
            .map(|c| codec_mime_to_proto(&c.mime_type))
            .unwrap_or(proto::VideoCodec::H264);

        let (self_dropped_tx, self_dropped_rx) = oneshot::channel();

        let handle_id = server.next_id();
        let stream = FfiEncodedVideoStream {
            handle_id,
            self_dropped_tx,
        };

        let handle = server.async_runtime.spawn(Self::encoded_video_stream_task(
            server,
            handle_id,
            rtc_receiver,
            capacity,
            drop_after_tap,
            self_dropped_rx,
            server.watch_handle_dropped(request.track_handle),
        ));
        server.watch_panic(handle);

        let info = proto::EncodedVideoStreamInfo {
            codec: codec.into(),
            payload_format: None,  // Default: unknown format
        };

        // Send codec config event
        let _ = server.send_event(
            proto::EncodedVideoStreamEvent {
                stream_handle: handle_id,
                message: Some(proto::encoded_video_stream_event::Message::CodecConfig(
                    proto::EncodedVideoCodecConfigReceived {
                        codec: codec.into(),
                        profile_level_id: None,
                        sps: None,
                        pps: None,
                        width: 0,
                        height: 0,
                    },
                )),
            }
            .into(),
        );

        server.store_handle(handle_id, stream);

        Ok(proto::OwnedEncodedVideoStream {
            handle: proto::FfiOwnedHandle { id: handle_id },
            info,
        })
    }

    async fn encoded_video_stream_task(
        server: &'static server::FfiServer,
        stream_handle: FfiHandleId,
        rtc_receiver: RtpReceiver,
        capacity: Option<usize>,
        drop_after_tap: bool,
        mut self_dropped_rx: oneshot::Receiver<()>,
        mut handle_dropped_rx: oneshot::Receiver<()>,
    ) {
        use webrtc_sys::rtp_receiver as sys_rr;

        // Get the C++ RtpReceiver handle
        let sys_receiver = rtc_receiver.sys_handle();

        // Create the native stream (pair: stream + sink)
        let (mut native_stream, sink) = NativeEncodedVideoStream::new(capacity);

        // Install the encoded tap on the receiver via CXX bridge
        sys_receiver.InstallEncodedTap(&sink, drop_after_tap);  // sink is SharedPtr

        loop {
            tokio::select! {
                _ = &mut self_dropped_rx => {
                    break;
                }
                _ = &mut handle_dropped_rx => {
                    break;
                }
                frame = native_stream.next() => {
                    let Some(frame) = frame else {
                        break;
                    };

                    // Get pointer/size before moving data into the handle
                    let data_ptr = frame.data.as_ptr() as u64;
                    let data_len = frame.data.len() as u32;

                    let buffer_id = server.next_id();
                    let buffer = EncodedVideoBufferHandle {
                        data: frame.data,
                    };
                    server.store_handle(buffer_id, buffer);

                    let owned_buffer = proto::OwnedEncodedVideoBuffer {
                        handle: proto::FfiOwnedHandle { id: buffer_id },
                        info: proto::EncodedVideoBufferInfo {
                            data_ptr,
                            size: data_len,
                        },
                    };

                    if let Err(err) = server.send_event(
                        proto::EncodedVideoStreamEvent {
                            stream_handle,
                            message: Some(
                                proto::encoded_video_stream_event::Message::FrameReceived(
                                    proto::EncodedVideoFrameReceived {
                                        buffer: owned_buffer,
                                        codec: frame.codec as i32,
                                        payload_format: frame.payload_format as i32,
                                        is_key_frame: frame.is_key_frame,
                                        width: frame.width,
                                        height: frame.height,
                                        timestamp_us: frame.timestamp_us,
                                        rtp_timestamp: frame.rtp_timestamp,
                                        rotation: frame.rotation as i32,
                                        qp: frame.qp,
                                        metadata: None,
                                    },
                                ),
                            ),
                        }
                        .into(),
                    ) {
                        server.drop_handle(buffer_id);
                        log::warn!("failed to send encoded video frame: {}", err);
                    }
                }
            }
        }

        // _tap is dropped here, removing it from the receiver's chain

        if let Err(err) = server.send_event(
            proto::EncodedVideoStreamEvent {
                stream_handle,
                message: Some(proto::encoded_video_stream_event::Message::Eos(
                    proto::VideoStreamEos {},
                )),
            }
            .into(),
        ) {
            log::warn!("failed to send encoded video stream EOS: {}", err);
        }
    }
}

fn codec_mime_to_proto(mime: &str) -> proto::VideoCodec {
    let lower = mime.to_lowercase();
    if lower.contains("vp8") {
        proto::VideoCodec::Vp8
    } else if lower.contains("vp9") {
        proto::VideoCodec::Vp9
    } else if lower.contains("av1") {
        proto::VideoCodec::Av1
    } else if lower.contains("h265") || lower.contains("hevc") {
        proto::VideoCodec::H265
    } else {
        proto::VideoCodec::H264
    }
}
