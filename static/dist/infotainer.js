const ADD = "add";
const GET = "get";
const DEL = "remove";
const DELIM = "::";
const TOKEN = btoa("placeholder");
let SEARCHFIELD = null;
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

function sub_request(subscriptionID, del=false) {
  if (!subscriptionID) {
    subscriptionID = SUBFIELD.options[SUBFIELD.selectedIndex].value;
  }
  return del ? ADD : DEL + DELIM + subscriptionID
}

function get_request(_data) {
  if (!_data) {
    _data = SEARCHFIELD.value;
    SEARCHFIELD.value = "";
  }
  return GET + DELIM + _data
}

function socket_send(_type, _data = null) {
  let payload = null;
  if (_type == GET) {
    payload = get_request(_data);
  } else if (_type == ADD) {
    payload = sub_request(_data);
  } else if (_type == DEL) {
    payload = sub_request(_data, del=true)
  } else {
    throw _type + " is unknown"
  }
  payload = TOKEN + "{" + payload + "}";
  if (socket == null) {
    throw "Not connected to a socket"
  }
  console.log("Sending\t" + payload);
  socket.send(payload);
}

document.addEventListener("DOMContentLoaded", function () {
  SEARCHFIELD = document.getElementById(GET);
  SUBFIELD = document.getElementById(ADD);
  connect();
  socket.onopen= function () {
    socket_send(ADD, "datasets");
  };
});