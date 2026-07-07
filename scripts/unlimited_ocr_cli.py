#!/usr/bin/env python3
"""
Unlimited-OCR CLI Wrapper for Claude Code
Usage: python unlimited_ocr_cli.py <image_path> [--prompt "document parsing."]
"""

import argparse
import sys
import torch
from transformers import AutoTokenizer, AutoModel

MODEL_NAME = "baidu/Unlimited-OCR"

# Module-level lazy singleton
_model = None
_tokenizer = None

def load_model():
    """Lazy load model once, cache globally"""
    global _model, _tokenizer
    if _model is None:
        print("[Unlimited-OCR] Loading model...", file=sys.stderr)
        _tokenizer = AutoTokenizer.from_pretrained(
            MODEL_NAME, trust_remote_code=True
        )
        _model = AutoModel.from_pretrained(
            MODEL_NAME,
            trust_remote_code=True,
            use_safetensors=True,
            torch_dtype=torch.bfloat16,
        )
        _model = _model.eval().cuda()
        print("[Unlimited-OCR] Model loaded.", file=sys.stderr)
    return _model, _tokenizer

def ocr_image(image_path: str, prompt: str = "<image>document parsing.") -> str:
    model, tokenizer = load_model()
    output = model.infer(
        tokenizer,
        prompt=prompt,
        image_file=image_path,
    )
    return output

def main():
    parser = argparse.ArgumentParser(description="Unlimited-OCR CLI for Claude Code")
    parser.add_argument("image", help="Path to image file")
    parser.add_argument("--prompt", default="<image>document parsing.", help="Prompt for OCR")
    parser.add_argument("--output", help="Output file (default: stdout)")
    args = parser.parse_args()

    result = ocr_image(args.image, args.prompt)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(result)
        print(f"[Unlimited-OCR] Written to {args.output}", file=sys.stderr)
    else:
        print(result)

if __name__ == "__main__":
    main()
