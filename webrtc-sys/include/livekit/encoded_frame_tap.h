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

#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <vector>

#include "api/frame_transformer_interface.h"
#include "api/scoped_refptr.h"
#include "livekit/webrtc.h"
#include "rtc_base/synchronization/mutex.h"
#include "rust/cxx.h"

namespace livekit_ffi {
class NativeEncodedFrameSink;
}
#include "webrtc-sys/src/encoded_frame_tap.rs.h"
namespace livekit_ffi {

class NativeEncodedFrameSink;

/// Refcounted buffer holding a copy of encoded frame payload.
/// Shared across multiple observers (stream + callback) to avoid redundant copies.
class EncodedVideoBuffer {
 public:
  EncodedVideoBuffer(const uint8_t* data, size_t size);
  ~EncodedVideoBuffer() = default;

  const uint8_t* data() const noexcept { return data_.data(); }
  size_t size() const noexcept { return data_.size(); }

 private:
  std::vector<uint8_t> data_;
};

/// Per-frame metadata captured by the tap.
struct EncodedFrameInfo {
  std::shared_ptr<EncodedVideoBuffer> buffer;
  int codec;
  bool is_key_frame;
  uint32_t width;
  uint32_t height;
  int64_t timestamp_us;
  uint32_t rtp_timestamp;
  int rotation;
  std::optional<int32_t> qp;
};

/// Receiver-side transformer that taps encoded frames before the decoder.
class EncodedFrameTapTransformer : public webrtc::FrameTransformerInterface {
 public:
  EncodedFrameTapTransformer();
  ~EncodedFrameTapTransformer() override = default;

  void AddSink(std::shared_ptr<NativeEncodedFrameSink> sink);
  void RemoveSink(std::shared_ptr<NativeEncodedFrameSink> sink);

  // FrameTransformerInterface
  void Transform(std::unique_ptr<webrtc::TransformableFrameInterface> frame) override;
  void RegisterTransformedFrameCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) override;
  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override;
  void UnregisterTransformedFrameCallback() override;
  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override;

 private:
  mutable webrtc::Mutex mutex_;
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback_
      RTC_GUARDED_BY(mutex_);
  std::vector<std::shared_ptr<NativeEncodedFrameSink>> sinks_
      RTC_GUARDED_BY(mutex_);
};

/// Sink that receives EncodedFrameInfo from the tap.
class NativeEncodedFrameSink {
 public:
  explicit NativeEncodedFrameSink(rust::Box<EncodedVideoSinkWrapper> observer);
  ~NativeEncodedFrameSink() = default;

  void OnEncodedFrame(const EncodedFrameInfo& info);

 private:
  rust::Box<EncodedVideoSinkWrapper> observer_;
};

std::shared_ptr<NativeEncodedFrameSink> new_native_encoded_frame_sink(
    rust::Box<EncodedVideoSinkWrapper> observer);

}  // namespace livekit_ffi
