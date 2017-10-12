# Chat example


## Server

Chat server listens for incoming tcp connections. Server can access several types of message:

  * `\list` - list all available rooms
  * `\join name` - join room, if room does not exist, create new one
  * `some message` - just string, send messsage to all peers in same room

## Client

Client connects to server. Reads input from stdin and sends to server.
