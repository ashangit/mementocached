#! /usr/bin/python

import protos.kv_pb2
import time
import socket
import random
import string

HOST = "127.0.0.1"
PORT = 6379

KEY_PREFIX = ''.join(random.choice(string.ascii_letters) for _ in range(10))

VALUE = bytes('nicovalue', 'utf-8')

def get(index):
    request: protos.kv_pb2.Request = protos.kv_pb2.Request()

    request.get.key = f'{KEY_PREFIX}-{index}'

    req = request.SerializeToString()
    bytes_len = request.ByteSize().to_bytes(8, 'big')
    req = bytes_len + req

    s.sendall(req)
    data = s.recv(1024)
    #reply = protos.kv_pb2.GetReply()
    #reply.ParseFromString(data)


def set(index):
    request: protos.kv_pb2.Request = protos.kv_pb2.Request()

    request.set.key = f'{KEY_PREFIX}-{index}'
    request.set.key = VALUE

    req = request.SerializeToString()
    bytes_len = request.ByteSize().to_bytes(8, 'big')
    req = bytes_len + req

    s.sendall(req)
    data = s.recv(1024)
    #reply = protos.kv_pb2.GetReply()
    #reply.ParseFromString(data)


if __name__ == '__main__':
    i = 0
    while True:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            try:
                s.connect((HOST, PORT))

                while True:
                    # Get
                    get(i)
                    # Set
                    set(i)
                    # Get
                    get(i)

                    #print(i)
                    i += 1
            except Exception as e:
                print(f"Issue {e}")
                time.sleep(1)
            finally:
                s.close()
