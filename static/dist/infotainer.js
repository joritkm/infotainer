const SUB = "subscription";
const MSG = "message";
let MSGFIELD = null;
let SUBFIELD = null;
let socket = null;


function connect() {
  disconnect();
  console.log("Connecting...");
  socket = new WebSocket("ws://localhost:8080/ws/");
}

function disconnect() {
  if (socket != null) {
    console.log("Disconnecting...");
    socket.onclose = function () {
      console.log("Disconnected.")
      socket = null;
    };
  }
}

function new_sub_request(_data) {
  if (!_data) {
    _data = SUBFIELD.options[SUBFIELD.selectedIndex].value;
  }
  return {
    type: SUB,
    data: _data
  }
}

function new_message(_data) {
  if (!_data) {
    _data = MSGFIELD.value;
  }
  MSGFIELD.value = "";
  return {
    type: MSG,
    data: _data
  }
}

function socket_send(_type, _data = null) {
  let payload = null;
  if (_type == "message") {
    payload = new_message(_data);
  } else if (_type == "subscription") {
    payload = new_sub_request(_data);
  } else {
    throw _type + " is unknown"
  }
  if (!payload.data) {
    throw "No data to send"
  } 
  if (socket == null) {
    throw "Not connected to a socket"
  }
  console.log("Sending " + payload.type + " request: \n" + payload.data);
  socket.send(JSON.stringify(payload));
}

document.addEventListener("DOMContentLoaded", function () {
  MSGFIELD = document.getElementById(MSG);
  SUBFIELD = document.getElementById(SUB);
  connect();
  socket.onopen= function () {
    socket_send(SUB, "datasets");
  };
});