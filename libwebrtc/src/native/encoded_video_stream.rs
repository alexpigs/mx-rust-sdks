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

use std::{
    collections::VecDeque,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

use cxx::SharedPtr;
use livekit_runtime::Stream;
use parking_lot::Mutex;
use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::{
    encoded_video_frame::{EncodedVideoCodecConfig, EncodedVideoFrame, EncodedPayloadFormat, VideoCodecType},
    video_frame::VideoRotation,
};
use webrtc_sys::encoded_frame_tap::{self as sys_tap, EncodedVideoSinkWrapper, EncodedVideoSink};

pub struct NativeEncodedVideoStream {
    frame_queue: Arc<EncodedVideoFrameQueue>,
    config_queue: Arc<EncodedVideoCodecConfigQueue>,
}

impl NativeEncodedVideoStream {
    pub fn new(
        capacity: Option<usize>,
    ) -> (Self, SharedPtr<sys_tap::ffi::NativeEncodedFrameSink>) {
        let frame_queue = Arc::new(EncodedVideoFrameQueue::new(capacity));
        let config_queue = Arc::new(EncodedVideoCodecConfigQueue::new());
        let observer = Arc::new(EncodedVideoTrackObserver {
            frame_queue: frame_queue.clone(),
        });
        let sink = sys_tap::ffi::new_native_encoded_frame_sink(Box::new(
            EncodedVideoSinkWrapper::new(observer.clone()),
        ));

        (
            Self { frame_queue, config_queue },
            sink,
        )
    }

    pub fn close(&mut self) {
        self.frame_queue.close();
        self.config_queue.close();
    }
}

impl Drop for NativeEncodedVideoStream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Stream for NativeEncodedVideoStream {
    type Item = EncodedVideoFrame;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        self.frame_queue.poll_recv(cx)
    }
}

struct EncodedVideoTrackObserver {
    frame_queue: Arc<EncodedVideoFrameQueue>,
}

impl sys_tap::EncodedVideoSink for EncodedVideoTrackObserver {
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
        let data = if data_ptr != 0 && size > 0 {
            unsafe { std::slice::from_raw_parts(data_ptr as *const u8, size as usize).to_vec() }
        } else {
            Vec::new()
        };

        let frame = EncodedVideoFrame {
            data,
            codec: VideoCodecType::from(codec),
            payload_format: EncodedPayloadFormat::WebRtc,
            is_key_frame,
            width,
            height,
            timestamp_us,
            rtp_timestamp,
            rotation: match rotation {
                0 => VideoRotation::VideoRotation0,
                90 => VideoRotation::VideoRotation90,
                180 => VideoRotation::VideoRotation180,
                270 => VideoRotation::VideoRotation270,
                _ => VideoRotation::VideoRotation0,
            },
            qp: if has_qp { Some(qp) } else { None },
            metadata: None,
        };

        self.frame_queue.push(frame);
    }
}

enum EncodedVideoFrameQueueKind {
    Bounded {
        producer: Mutex<Producer<EncodedVideoFrame>>,
        consumer: Mutex<Consumer<EncodedVideoFrame>>,
    },
    Unbounded {
        frames: Mutex<VecDeque<EncodedVideoFrame>>,
    },
}

struct EncodedVideoFrameQueue {
    kind: EncodedVideoFrameQueueKind,
    closed: AtomicBool,
    dropped_frames: AtomicU64,
    waker: Mutex<Option<Waker>>,
}

impl EncodedVideoFrameQueue {
    fn new(capacity: Option<usize>) -> Self {
        let kind = match capacity.filter(|&c| c > 0) {
            Some(capacity) => {
                let (producer, consumer) = RingBuffer::new(capacity);
                EncodedVideoFrameQueueKind::Bounded {
                    producer: Mutex::new(producer),
                    consumer: Mutex::new(consumer),
                }
            }
            None => EncodedVideoFrameQueueKind::Unbounded {
                frames: Mutex::new(VecDeque::new()),
            },
        };

        Self {
            kind,
            closed: AtomicBool::new(false),
            dropped_frames: AtomicU64::new(0),
            waker: Mutex::new(None),
        }
    }

    fn push(&self, frame: EncodedVideoFrame) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }

        match &self.kind {
            EncodedVideoFrameQueueKind::Bounded { producer, consumer } => {
                let mut prod = producer.lock();
                match prod.push(frame) {
                    Ok(()) => {}
                    Err(PushError::Full(mut frame)) => {
                        let dropped = consumer.lock().pop().is_ok();
                        if dropped {
                            self.dropped_frames.fetch_add(1, Ordering::Relaxed);
                        }
                        let _ = prod.push(frame);
                    }
                }
            }
            EncodedVideoFrameQueueKind::Unbounded { frames } => {
                frames.lock().push_back(frame);
            }
        }

        self.wake();
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        match &self.kind {
            EncodedVideoFrameQueueKind::Bounded { consumer, .. } => {
                let mut c = consumer.lock();
                while c.pop().is_ok() {}
            }
            EncodedVideoFrameQueueKind::Unbounded { frames } => {
                frames.lock().clear();
            }
        }
        self.wake();
    }

    fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<EncodedVideoFrame>> {
        if let Some(frame) = self.try_pop() {
            return Poll::Ready(Some(frame));
        }

        if self.closed.load(Ordering::Acquire) {
            return Poll::Ready(None);
        }

        *self.waker.lock() = Some(cx.waker().clone());

        if let Some(frame) = self.try_pop() {
            self.waker.lock().take();
            Poll::Ready(Some(frame))
        } else if self.closed.load(Ordering::Acquire) {
            Poll::Ready(None)
        } else {
            Poll::Pending
        }
    }

    fn try_pop(&self) -> Option<EncodedVideoFrame> {
        match &self.kind {
            EncodedVideoFrameQueueKind::Bounded { consumer, .. } => consumer.lock().pop().ok(),
            EncodedVideoFrameQueueKind::Unbounded { frames } => frames.lock().pop_front(),
        }
    }

    fn wake(&self) {
        if let Some(waker) = self.waker.lock().take() {
            waker.wake();
        }
    }
}

struct EncodedVideoCodecConfigQueue {
    config: Mutex<Option<EncodedVideoCodecConfig>>,
    closed: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl EncodedVideoCodecConfigQueue {
    fn new() -> Self {
        Self {
            config: Mutex::new(None),
            closed: AtomicBool::new(false),
            waker: Mutex::new(None),
        }
    }

    fn close(&self) {
        self.closed.store(true, Ordering::Release);
        if let Some(waker) = self.waker.lock().take() {
            waker.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoded_video_frame::{EncodedPayloadFormat, VideoCodecType};
    use crate::video_frame::VideoRotation;

    fn test_frame(ts: i64) -> EncodedVideoFrame {
        EncodedVideoFrame {
            data: vec![0u8; 100],
            codec: VideoCodecType::H264,
            payload_format: EncodedPayloadFormat::WebRtc,
            is_key_frame: false,
            width: 1920,
            height: 1080,
            timestamp_us: ts,
            rtp_timestamp: ts as u32,
            rotation: VideoRotation::VideoRotation0,
            qp: None,
            metadata: None,
        }
    }

    #[test]
    fn bounded_queue_preserves_fifo_order() {
        let queue = EncodedVideoFrameQueue::new(Some(3));
        queue.push(test_frame(1));
        queue.push(test_frame(2));
        queue.push(test_frame(3));

        assert_eq!(queue.try_pop().unwrap().timestamp_us, 1);
        assert_eq!(queue.try_pop().unwrap().timestamp_us, 2);
        assert_eq!(queue.try_pop().unwrap().timestamp_us, 3);
        assert!(queue.try_pop().is_none());
    }

    #[test]
    fn bounded_queue_drops_oldest_when_full() {
        let queue = EncodedVideoFrameQueue::new(Some(2));
        queue.push(test_frame(1));
        queue.push(test_frame(2));
        queue.push(test_frame(3));

        assert_eq!(queue.try_pop().unwrap().timestamp_us, 2);
        assert_eq!(queue.try_pop().unwrap().timestamp_us, 3);
        assert!(queue.try_pop().is_none());
    }

    #[test]
    fn unbounded_queue_retains_all_frames() {
        let queue = EncodedVideoFrameQueue::new(None);
        for ts in 1..=10 {
            queue.push(test_frame(ts));
        }
        for ts in 1..=10 {
            assert_eq!(queue.try_pop().unwrap().timestamp_us, ts);
        }
    }

    #[test]
    fn close_clears_buffer_and_rejects_pushes() {
        let queue = EncodedVideoFrameQueue::new(Some(5));
        queue.push(test_frame(1));
        queue.close();
        queue.push(test_frame(2));
        assert!(queue.try_pop().is_none());
    }
}
