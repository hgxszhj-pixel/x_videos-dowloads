#!/usr/bin/env python3
"""
Unlimited-OCR via ModelScope
Uses PaddlePaddle/Unlimited-OCR from ModelScope (domestic, fast).
"""

import argparse
import sys

def ocr_image(image_path: str, mode: str = "gundam", prompt: str = "document parsing.") -> str:
    """
    Call Unlimited-OCR via ModelScope pipeline.
    """
    from modelscope.pipelines import pipeline
    from modelscope.utils.constant import Tasks

    ocr_pipeline = pipeline(
        task=Tasks.ocr,
        model='PaddlePaddle/Unlimited-OCR',
        model_revision='v1.0.0'
    )

    result = ocr_pipeline(
        input=image_path,
        mode=mode,
        prompt=prompt
    )

    if isinstance(result, dict):
        return result.get('text', str(result))
    return str(result)

def main():
    parser = argparse.ArgumentParser(description="Unlimited-OCR via ModelScope")
    parser.add_argument("image", help="Path to image file")
    parser.add_argument("--mode", default="gundam", help="Mode: gundam or base (default: gundam)")
    parser.add_argument("--prompt", default="document parsing.", help="OCR prompt")
    parser.add_argument("--output", help="Output file (default: stdout)")
    args = parser.parse_args()

    print("[Unlimited-OCR] Loading ModelScope model...", file=sys.stderr)
    result = ocr_image(args.image, mode=args.mode, prompt=args.prompt)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(result)
        print(f"[Unlimited-OCR] Written to {args.output}", file=sys.stderr)
    else:
        print(result)

if __name__ == "__main__":
    main()
