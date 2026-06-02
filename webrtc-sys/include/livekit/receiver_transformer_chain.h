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

#include <map>
#include <memory>
#include <vector>

#include "api/frame_transformer_interface.h"
#include "api/scoped_refptr.h"
#include "livekit/webrtc.h"
#include "rtc_base/synchronization/mutex.h"

namespace livekit_ffi {

class ReceiverTransformerChain : public webrtc::FrameTransformerInterface,
                                public webrtc::TransformedFrameCallback {
 public:
  enum class Priority {
    kPacketTrailer = 0,
    kFrameCryptor = 1,
    kEncodedTapPreDecrypt = 2,
    kEncodedTapPostDecrypt = 3,
  };

  ReceiverTransformerChain() = default;
  ~ReceiverTransformerChain() override = default;

  void AddTransformer(
      Priority priority,
      webrtc::scoped_refptr<webrtc::FrameTransformerInterface> transformer);
  void RemoveTransformer(Priority priority);

  // FrameTransformerInterface
  void Transform(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override;
  void RegisterTransformedFrameCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) override;
  void RegisterTransformedFrameSinkCallback(
      webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
      uint32_t ssrc) override;
  void UnregisterTransformedFrameCallback() override;
  void UnregisterTransformedFrameSinkCallback(uint32_t ssrc) override;

  // TransformedFrameCallback
  void OnTransformedFrame(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override;

 private:
  void RebuildChain();
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> BuildChainFrom(
      std::vector<webrtc::scoped_refptr<webrtc::FrameTransformerInterface>>::iterator begin,
      std::vector<webrtc::scoped_refptr<webrtc::FrameTransformerInterface>>::iterator end);

  mutable webrtc::Mutex mutex_;
  std::map<Priority, webrtc::scoped_refptr<webrtc::FrameTransformerInterface>> transformers_
      RTC_GUARDED_BY(mutex_);
  webrtc::scoped_refptr<webrtc::TransformedFrameCallback> output_callback_
      RTC_GUARDED_BY(mutex_);
};

}  // namespace livekit_ffi
