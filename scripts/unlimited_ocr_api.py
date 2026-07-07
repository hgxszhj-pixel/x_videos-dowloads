#!/usr/bin/env python3
"""
Unlimited-OCR via Hugging Face Spaces using gradio_client.
健壮版：重试 + 长超时 + 详细日志
"""

import argparse
import sys
import time
from gradio_client import Client

HF_SPACE = "baidu/Unlimited-OCR"

def ocr_image(image_path: str, mode: str = "gundam", prompt: str = "document parsing.", retries: int = 3) -> str:
    """
    Call Unlimited-OCR via HF Space gradio API with retries.
    """
    last_err = None
    for attempt in range(retries):
        try:
            print(f"[Attempt {attempt+1}/{retries}] Connecting to HF Space...", file=sys.stderr)
            client = Client(HF_SPACE, httpx_kwargs={'timeout': 600})
            print(f"[Attempt {attempt+1}] Calling /run_ocr...", file=sys.stderr)
            result = client.predict(
                image_path=image_path,
                mode=mode,
                prompt=prompt,
                api_name="/run_ocr"
            )
            return str(result)
        except Exception as e:
            last_err = e
            print(f"[Attempt {attempt+1}] Failed: {e}", file=sys.stderr)
            if attempt < retries - 1:
                print(f"[Attempt {attempt+1}] Retrying in 10s...", file=sys.stderr)
                time.sleep(10)
    raise RuntimeError(f"All {retries} attempts failed: {last_err}")

def main():
    parser = argparse.ArgumentParser(description="Unlimited-OCR via HF Space (with retries)")
    parser.add_argument("image", help="Path to image file")
    parser.add_argument("--mode", default="gundam", help="Mode: gundam or base (default: gundam)")
    parser.add_argument("--prompt", default="document parsing.", help="OCR prompt")
    parser.add_argument("--output", help="Output file (default: stdout)")
    parser.add_argument("--retries", type=int, default=3, help="Number of retries (default: 3)")
    args = parser.parse_args()

    print(f"[Unlimited-OCR] Starting OCR on {args.image}", file=sys.stderr)
    result = ocr_image(args.image, mode=args.mode, prompt=args.prompt, retries=args.retries)

    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(result)
        print(f"[Unlimited-OCR] Written to {args.output}", file=sys.stderr)
    else:
        print(result)

if __name__ == "__main__":
    main()
