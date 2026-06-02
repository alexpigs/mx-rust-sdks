/*
 * Copyright 2026 LiveKit, Inc.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#include "livekit/encoded_frame_tap.h"

#include "api/video/encoded_image.h"
#include "api/video/video_codec_type.h"
#include "webrtc-sys/src/encoded_frame_tap.rs.h"

namespace livekit_ffi {

EncodedVideoBuffer::EncodedVideoBuffer(const uint8_t* data, size_t size)
    : data_(data, data + size) {}

EncodedFrameTapTransformer::EncodedFrameTapTransformer() = default;

void EncodedFrameTapTransformer::AddSink(
    std::shared_ptr<NativeEncodedFrameSink> sink) {
  webrtc::MutexLock lock(&mutex_);
  sinks_.push_back(std::move(sink));
}

void EncodedFrameTapTransformer::RemoveSink(
    std::shared_ptr<NativeEncodedFrameSink> sink) {
  webrtc::MutexLock lock(&mutex_);
  sinks_.erase(
      std::remove(sinks_.begin(), sinks_.end(), sink),
      sinks_.end());
}

void EncodedFrameTapTransformer::Transform(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  webrtc::MutexLock lock(&mutex_);

  auto data_view = frame->GetData();
  auto buffer = std::make_shared<EncodedVideoBuffer>(
      data_view.data(), data_view.size());

  EncodedFrameInfo info;
  info.buffer = buffer;

  // Use the metadata available from TransformableFrameInterface.
  // Codec/width/height/keyframe info is not directly exposed by this
  // WebRTC API version — defaults are set and can be enriched later.
  info.codec = -1;
  info.is_key_frame = false;
  info.width = 0;
  info.height = 0;
  info.rtp_timestamp = frame->GetTimestamp();
  info.rotation = 0;

  for (auto& sink : sinks_) {
    sink->OnEncodedFrame(info);
  }

  if (callback_) {
    callback_->OnTransformedFrame(std::move(frame));
  }
}

void EncodedFrameTapTransformer::RegisterTransformedFrameCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = std::move(callback);
}

void EncodedFrameTapTransformer::RegisterTransformedFrameSinkCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = std::move(callback);
}

void EncodedFrameTapTransformer::UnregisterTransformedFrameCallback() {
  webrtc::MutexLock lock(&mutex_);
  callback_ = nullptr;
}

void EncodedFrameTapTransformer::UnregisterTransformedFrameSinkCallback(
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  callback_ = nullptr;
}

NativeEncodedFrameSink::NativeEncodedFrameSink(
    rust::Box<EncodedVideoSinkWrapper> observer)
    : observer_(std::move(observer)) {}

void NativeEncodedFrameSink::OnEncodedFrame(const EncodedFrameInfo& info) {
  observer_->on_encoded_frame(
      reinterpret_cast<uint64_t>(info.buffer->data()),
      static_cast<uint32_t>(info.buffer->size()),
      info.codec,
      info.is_key_frame,
      info.width,
      info.height,
      info.timestamp_us,
      info.rtp_timestamp,
      info.rotation,
      info.qp.value_or(-1),
      info.qp.has_value());
}

std::shared_ptr<NativeEncodedFrameSink> new_native_encoded_frame_sink(
    rust::Box<EncodedVideoSinkWrapper> observer) {
  return std::make_shared<NativeEncodedFrameSink>(std::move(observer));
}

}  // namespace livekit_ffi
