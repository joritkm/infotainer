const MAGICTOKEN = "magictoken"
const TOKENURL = "http://localhost:8080/id";
const SOCKETURL = "ws://localhost:8080/ws/"
const ADD = "add";
const GET = "get";
const DEL = "remove";
const DELIM = "::";
const storage = window.sessionStorage;
let searchfield = null;
let subfield = null;
let sessionStatusDisplay = null;
let socket = null;


function connect() {
  disconnect();
  console.log("Connecting...");
  socket = new WebSocket(SOCKETURL);
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
    subscriptionID = subfield.options[subfield.selectedIndex].value;
  }
  return del ? DEL : ADD + DELIM + subscriptionID
}

function get_request(_data) {
  if (!_data) {
    _data = searchfield.value;
    searchfield.value = "";
  }
  return GET + DELIM + _data
}

function client_id() {
  const session = JSON.parse(storage.getItem("clientSession"));
  return session.client.uid;
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
  payload = client_id() + "|" + payload;
  if (socket == null) {
    throw "Not connected to a socket"
  }
  console.log("Sending\t" + payload);
  socket.send(payload);
}

function setSessionStatus(clientSession) {
  let sessionString = "ID: " + clientSession.client.uid;
  console.log(sessionString);
  sessionStatusDisplay.textContent = sessionString;
}

document.addEventListener("DOMContentLoaded", async function () {
  searchfield = document.getElementById(GET);
  subfield = document.getElementById(ADD);
  sessionStatusDisplay = document.getElementById("clientSessionStatus");
  if (!storage.getItem('clientSession')) {
    let tokenResp = await fetch(TOKENURL, {
      method: 'POST',
      headers: {
        'X-Auth-Token': MAGICTOKEN
      }
    });
    const client = await tokenResp.json();
    storage.setItem('clientSession', JSON.stringify(client));
    setSessionStatus(client);
  }
  connect();
  socket.onopen= function () {
    socket_send(ADD, "datasets");
  };
});