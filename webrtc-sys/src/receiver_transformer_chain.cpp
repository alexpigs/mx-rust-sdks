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

#include "livekit/receiver_transformer_chain.h"

#include "api/make_ref_counted.h"

namespace livekit_ffi {

// Bridges the gap between TransformedFrameCallback and FrameTransformerInterface.
// Each transformer in the chain registers this as its callback; when invoked,
// it forwards the frame to the next transformer's Transform().
class ChainStep : public webrtc::TransformedFrameCallback {
 public:
  explicit ChainStep(webrtc::scoped_refptr<webrtc::FrameTransformerInterface> next)
      : next_(std::move(next)) {}

  void OnTransformedFrame(
      std::unique_ptr<webrtc::TransformableFrameInterface> frame) override {
    next_->Transform(std::move(frame));
  }

 private:
  webrtc::scoped_refptr<webrtc::FrameTransformerInterface> next_;
};

void ReceiverTransformerChain::AddTransformer(
    Priority priority,
    webrtc::scoped_refptr<webrtc::FrameTransformerInterface> transformer) {
  webrtc::MutexLock lock(&mutex_);
  transformers_[priority] = std::move(transformer);
  RebuildChain();
}

void ReceiverTransformerChain::RemoveTransformer(Priority priority) {
  webrtc::MutexLock lock(&mutex_);
  transformers_.erase(priority);
  RebuildChain();
}

void ReceiverTransformerChain::Transform(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  webrtc::MutexLock lock(&mutex_);
  if (transformers_.empty()) {
    if (output_callback_) {
      output_callback_->OnTransformedFrame(std::move(frame));
    }
    return;
  }
  transformers_.begin()->second->Transform(std::move(frame));
}

void ReceiverTransformerChain::RegisterTransformedFrameCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback) {
  webrtc::MutexLock lock(&mutex_);
  output_callback_ = std::move(callback);
  RebuildChain();
}

void ReceiverTransformerChain::RegisterTransformedFrameSinkCallback(
    webrtc::scoped_refptr<webrtc::TransformedFrameCallback> callback,
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  output_callback_ = std::move(callback);
  if (!transformers_.empty()) {
    RebuildChain();
  }
}

void ReceiverTransformerChain::UnregisterTransformedFrameCallback() {
  webrtc::MutexLock lock(&mutex_);
  output_callback_ = nullptr;
  RebuildChain();
}

void ReceiverTransformerChain::UnregisterTransformedFrameSinkCallback(
    uint32_t ssrc) {
  webrtc::MutexLock lock(&mutex_);
  for (auto& [_, t] : transformers_) {
    t->UnregisterTransformedFrameSinkCallback(ssrc);
  }
}

void ReceiverTransformerChain::OnTransformedFrame(
    std::unique_ptr<webrtc::TransformableFrameInterface> frame) {
  webrtc::MutexLock lock(&mutex_);
  if (output_callback_) {
    output_callback_->OnTransformedFrame(std::move(frame));
  }
}

void ReceiverTransformerChain::RebuildChain() {
  if (transformers_.empty()) {
    return;
  }

  auto it = transformers_.begin();
  auto last = std::prev(transformers_.end());

  for (; it != last; ++it) {
    auto next = std::next(it);
    auto step = webrtc::make_ref_counted<ChainStep>(next->second);
    it->second->RegisterTransformedFrameCallback(step);
  }

  if (output_callback_) {
    last->second->RegisterTransformedFrameCallback(output_callback_);
  } else {
    last->second->RegisterTransformedFrameCallback(nullptr);
  }
}

webrtc::scoped_refptr<webrtc::FrameTransformerInterface>
ReceiverTransformerChain::BuildChainFrom(
    std::vector<webrtc::scoped_refptr<webrtc::FrameTransformerInterface>>::iterator begin,
    std::vector<webrtc::scoped_refptr<webrtc::FrameTransformerInterface>>::iterator end) {
  // Unused — chain is rebuilt in-place via RebuildChain()
  if (begin == end) return nullptr;
  return *begin;
}

}  // namespace livekit_ffi
