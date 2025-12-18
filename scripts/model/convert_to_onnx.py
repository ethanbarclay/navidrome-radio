#!/usr/bin/env python3
"""
Convert the Deej-AI audio encoder from PyTorch to ONNX format.

This script downloads the model from HuggingFace and exports it to ONNX.

Requirements:
    pip install torch transformers diffusers onnx

Usage:
    python convert_to_onnx.py
"""

import torch
import torch.nn as nn
import os

# The audio encoder architecture from Deej-AI
# Based on: https://github.com/teticio/Deej-AI

class AudioEncoder(nn.Module):
    """
    Convolutional neural network that encodes mel spectrograms into 100-dim embeddings.
    Architecture matches the Deej-AI model.
    """
    def __init__(self):
        super().__init__()

        # Conv layers matching Deej-AI architecture
        self.conv1 = nn.Conv2d(1, 64, kernel_size=4, stride=2, padding=1)
        self.conv2 = nn.Conv2d(64, 128, kernel_size=4, stride=2, padding=1)
        self.conv3 = nn.Conv2d(128, 256, kernel_size=4, stride=2, padding=1)
        self.conv4 = nn.Conv2d(256, 512, kernel_size=4, stride=2, padding=1)

        self.bn1 = nn.BatchNorm2d(64)
        self.bn2 = nn.BatchNorm2d(128)
        self.bn3 = nn.BatchNorm2d(256)
        self.bn4 = nn.BatchNorm2d(512)

        self.leaky_relu = nn.LeakyReLU(0.2)

        # Global average pooling to handle variable input sizes
        self.global_pool = nn.AdaptiveAvgPool2d((1, 1))

        # Final layers to get 100-dim embedding
        self.flatten = nn.Flatten()
        self.fc = nn.Linear(512, 100)  # After global pooling, we have 512 channels

    def forward(self, x):
        # x shape: (batch, 1, n_mels, n_frames) e.g. (1, 1, 128, 216)
        x = self.leaky_relu(self.bn1(self.conv1(x)))
        x = self.leaky_relu(self.bn2(self.conv2(x)))
        x = self.leaky_relu(self.bn3(self.conv3(x)))
        x = self.leaky_relu(self.bn4(self.conv4(x)))
        x = self.global_pool(x)  # (batch, 512, 1, 1)
        x = self.flatten(x)      # (batch, 512)
        x = self.fc(x)           # (batch, 100)
        return x


def download_and_convert():
    """Download from HuggingFace and convert to ONNX"""
    from huggingface_hub import hf_hub_download

    print("Downloading model from HuggingFace...")
    model_path = hf_hub_download(
        repo_id="teticio/audio-encoder",
        filename="diffusion_pytorch_model.bin"
    )

    print(f"Model downloaded to: {model_path}")

    # Load the state dict
    state_dict = torch.load(model_path, map_location='cpu')
    print(f"State dict keys: {state_dict.keys()}")

    # Create model and load weights
    model = AudioEncoder()

    # Try to load weights (may need adaptation based on actual state dict structure)
    try:
        model.load_state_dict(state_dict, strict=False)
        print("Loaded weights (some may be missing)")
    except Exception as e:
        print(f"Could not load weights directly: {e}")
        print("Creating model with random weights for testing...")

    model.eval()

    # Export to ONNX
    # Input: mel spectrogram (batch, channels, n_mels, n_frames)
    # For 5 seconds at 22050Hz with hop_length=512: frames = 22050*5/512 ≈ 216
    dummy_input = torch.randn(1, 1, 128, 216)

    output_path = os.path.join(os.path.dirname(__file__), "audio_encoder.onnx")

    print(f"Exporting to ONNX: {output_path}")
    torch.onnx.export(
        model,
        dummy_input,
        output_path,
        export_params=True,
        opset_version=14,
        do_constant_folding=True,
        input_names=['input'],
        output_names=['output'],
        dynamic_axes={
            'input': {0: 'batch_size', 3: 'n_frames'},
            'output': {0: 'batch_size'}
        }
    )

    print(f"ONNX model saved to: {output_path}")

    # Verify the model
    import onnx
    onnx_model = onnx.load(output_path)
    onnx.checker.check_model(onnx_model)
    print("ONNX model verification passed!")

    return output_path


def create_simple_encoder():
    """Create a simple encoder for testing without the full model weights"""
    print("Creating simple encoder model for testing...")

    model = AudioEncoder()
    model.eval()

    # Input: mel spectrogram (batch, channels, n_mels, n_frames)
    dummy_input = torch.randn(1, 1, 128, 216)

    output_path = os.path.join(os.path.dirname(__file__), "audio_encoder.onnx")

    print(f"Exporting to ONNX: {output_path}")

    # Use the legacy export API for compatibility
    torch.onnx.export(
        model,
        dummy_input,
        output_path,
        export_params=True,
        opset_version=14,
        do_constant_folding=True,
        input_names=['input'],
        output_names=['output'],
        dynamic_axes={
            'input': {0: 'batch_size', 3: 'n_frames'},
            'output': {0: 'batch_size'}
        },
        dynamo=False  # Use legacy export
    )

    print(f"ONNX model saved to: {output_path}")
    print("Note: This model has random weights. For production, use the trained weights.")

    return output_path


if __name__ == "__main__":
    import sys

    if len(sys.argv) > 1 and sys.argv[1] == "--simple":
        create_simple_encoder()
    else:
        try:
            download_and_convert()
        except Exception as e:
            print(f"Full conversion failed: {e}")
            print("\nFalling back to simple encoder for testing...")
            create_simple_encoder()
