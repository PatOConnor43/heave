GET {{ baseurl }}/user/login
Authorization: Bearer {{ authorization }}
Prefer: code=400

[QueryStringParams]
username:
password:


HTTP 400
