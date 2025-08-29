#!/usr/bin/env python3
"""
Capture WebSocket frames from kubectl port-forward to understand the protocol
"""

import asyncio
import websockets
import sys
import struct

async def handle_client(websocket, path):
    """Handle incoming WebSocket connection from kubectl"""
    print(f"Client connected to {path}")
    
    # Accept the connection with SPDY protocol
    print(f"Subprotocols: {websocket.subprotocol}")
    
    try:
        # Send initial acknowledgment frames
        # Stream 0 (data) and stream 1 (error)
        await websocket.send(bytes([0x00, 0x00, 0x00, 0x00]))  # Stream 0 ack
        await websocket.send(bytes([0x01, 0x00, 0x00, 0x00]))  # Stream 1 ack
        print("Sent initial acknowledgment frames")
        
        frame_count = 0
        async for message in websocket:
            frame_count += 1
            if isinstance(message, bytes):
                print(f"\nFrame {frame_count}: Binary message ({len(message)} bytes)")
                print(f"  Hex: {message.hex(' ', 1)}")
                print(f"  Raw: {list(message)}")
                
                # Try to parse as SPDY frame
                if len(message) >= 4:
                    stream_id = message[0]
                    flags = message[1]
                    length = struct.unpack('>H', message[2:4])[0] if len(message) >= 4 else 0
                    data = message[4:] if len(message) > 4 else b''
                    
                    print(f"  Parsed: stream_id={stream_id}, flags=0x{flags:02x}, length={length}, data_len={len(data)}")
                    if data and len(data) <= 100:
                        try:
                            print(f"  Data as string: {data.decode('utf-8', errors='replace')}")
                        except:
                            pass
                            
                # Check for special patterns
                if len(message) == 2:
                    if message[0] == 0x80 and message[1] == 0x03:
                        print("  -> SPDY control frame")
                    else:
                        print(f"  -> Unknown 2-byte frame")
                        
            else:
                print(f"\nFrame {frame_count}: Text message: {message}")
                
    except websockets.exceptions.ConnectionClosed:
        print("\nConnection closed by client")
    except Exception as e:
        print(f"\nError: {e}")

async def main():
    # Start WebSocket server on port 8765
    print("Starting WebSocket server on port 8765...")
    print("Run: kubectl port-forward pod/nginx 8080:80 --server=http://localhost:8765")
    
    async with websockets.serve(
        handle_client, 
        "localhost", 
        8765,
        subprotocols=["SPDY/3.1+portforward.k8s.io", "portforward.k8s.io"]
    ):
        await asyncio.Future()  # Run forever

if __name__ == "__main__":
    asyncio.run(main())