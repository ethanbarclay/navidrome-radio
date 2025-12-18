#!/usr/bin/env python3
"""
Export the audiodiffusion AudioEncoder to ONNX format.

The teticio/audio-encoder model expects:
- n_mels = 96
- slice_size = 216
- Input shape: (batch, 1, 96, 216)

Architecture uses:
- 3 SeparableConv2D blocks (1→32→64→128) with BatchNorm and MaxPool
- Dense block (41472 → 1024) with BatchNorm
- Final embedding layer (1024 → 100)
"""

import torch
import torch.nn as nn
from huggingface_hub import hf_hub_download


class SeparableConv2d(nn.Module):
    """Depthwise separable convolution."""
    def __init__(self, in_channels, out_channels, kernel_size=3, padding=1):
        super().__init__()
        self.depthwise = nn.Conv2d(
            in_channels, in_channels, kernel_size,
            padding=padding, groups=in_channels, bias=False
        )
        self.pointwise = nn.Conv2d(in_channels, out_channels, 1, bias=True)

    def forward(self, x):
        x = self.depthwise(x)
        x = self.pointwise(x)
        return x


class ConvBlock(nn.Module):
    """Conv block with separable conv, batch norm, relu, and max pool."""
    def __init__(self, in_channels, out_channels):
        super().__init__()
        self.sep_conv = SeparableConv2d(in_channels, out_channels)
        self.batch_norm = nn.BatchNorm2d(out_channels)
        self.relu = nn.ReLU()
        self.maxpool = nn.MaxPool2d(2)

    def forward(self, x):
        x = self.sep_conv(x)
        x = self.batch_norm(x)
        x = self.relu(x)
        x = self.maxpool(x)
        return x


class DenseBlock(nn.Module):
    """Dense block with linear, batch norm, and relu."""
    def __init__(self, in_features, out_features):
        super().__init__()
        self.dense = nn.Linear(in_features, out_features)
        self.batch_norm = nn.BatchNorm1d(out_features)
        self.relu = nn.ReLU()

    def forward(self, x):
        x = self.dense(x)
        x = self.batch_norm(x)
        x = self.relu(x)
        return x


class AudioEncoder(nn.Module):
    """
    AudioEncoder from teticio/audio-encoder.
    Matches the architecture in the diffusion_pytorch_model.bin weights.
    """
    def __init__(self):
        super().__init__()

        # 3 conv blocks: 1→32→64→128
        self.conv_blocks = nn.ModuleList([
            ConvBlock(1, 32),
            ConvBlock(32, 64),
            ConvBlock(64, 128),
        ])

        # After 3 maxpools on 96x216: 12x27x128 = 41472
        self.flatten = nn.Flatten()
        self.dense_block = DenseBlock(41472, 1024)
        self.embedding = nn.Linear(1024, 100)

    def forward(self, x):
        for conv_block in self.conv_blocks:
            x = conv_block(x)
        x = self.flatten(x)
        x = self.dense_block(x)
        x = self.embedding(x)
        return x


def main():
    print("Creating AudioEncoder model...")
    model = AudioEncoder()

    print("Downloading weights from HuggingFace...")
    weights_path = hf_hub_download('teticio/audio-encoder', 'diffusion_pytorch_model.bin')
    print(f"Downloaded to: {weights_path}")

    state_dict = torch.load(weights_path, map_location='cpu', weights_only=True)

    print("Loading weights...")
    model.load_state_dict(state_dict)
    print("Weights loaded successfully!")

    model.eval()

    # Test forward pass
    print("\nTesting forward pass...")
    dummy_input = torch.randn(1, 1, 96, 216)
    with torch.no_grad():
        output = model(dummy_input)
    print(f"Input shape: {dummy_input.shape}")
    print(f"Output shape: {output.shape}")
    print(f"Output sample: {output[0, :5].tolist()}")

    # Export to ONNX using legacy JIT export (more compatible)
    output_path = "models/audio_encoder_correct.onnx"
    print(f"\nExporting to {output_path}...")

    # Use the legacy export path
    torch.onnx.export(
        model,
        dummy_input,
        output_path,
        input_names=["mel_spectrogram"],
        output_names=["embedding"],
        dynamic_axes={
            "mel_spectrogram": {0: "batch_size"},
            "embedding": {0: "batch_size"}
        },
        opset_version=14,
        dynamo=False  # Use legacy JIT export
    )

    print(f"Successfully exported to {output_path}")

    # Verify the export
    import onnx
    onnx_model = onnx.load(output_path)
    onnx.checker.check_model(onnx_model)
    print("ONNX model validated successfully!")

    # Print input/output info
    print("\nModel inputs:")
    for inp in onnx_model.graph.input:
        dims = [d.dim_value if d.dim_value else d.dim_param for d in inp.type.tensor_type.shape.dim]
        print(f"  {inp.name}: {dims}")

    print("\nModel outputs:")
    for out in onnx_model.graph.output:
        dims = [d.dim_value if d.dim_value else d.dim_param for d in out.type.tensor_type.shape.dim]
        print(f"  {out.name}: {dims}")

    # Skip ONNX runtime verification (not available for Python 3.14)
    # The Rust code will use the ort crate for inference

    print("\n✓ Export complete! New model ready at models/audio_encoder_correct.onnx")


if __name__ == "__main__":
    main()
