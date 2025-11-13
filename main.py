import mss
import cv2
import numpy as np
import time
import keyboard
from typing import cast

def record_screen(output_file='screen_recording.mp4', fps=10):
    with mss.mss() as sct:
        monitor = sct.monitors[1]  # Primary monitor
        width = monitor['width']
        height = monitor['height']
        fourcc_fn = getattr(cv2, 'VideoWriter_fourcc', None)
        if callable(fourcc_fn):
            try:
                fourcc = cast(int, fourcc_fn(*'mp4v'))
            except Exception:
                # build fourcc int manually as fallback
                fourcc = (ord('m') | (ord('p') << 8) | (ord('4') << 16) | (ord('v') << 24))
        else:
            # build fourcc int manually as fallback
            fourcc = (ord('m') | (ord('p') << 8) | (ord('4') << 16) | (ord('v') << 24))
            fourcc = (ord('m') | (ord('p') << 8) | (ord('4') << 16) | (ord('v') << 24))
        out = cv2.VideoWriter(output_file, fourcc, fps, (width, height))
        
        print("Recording... Press 'q' to stop.")
        while True:
            img = sct.grab(monitor)
            frame = np.array(img)
            frame = cv2.cvtColor(frame, cv2.COLOR_BGRA2BGR)
            out.write(frame)
            
            if keyboard.is_pressed('q'):
                break
            time.sleep(1 / fps)
        
        out.release()
        print("Recording saved to", output_file)

if __name__ == "__main__":
    record_screen()