GET {{ baseurl }}/pet/findByStatus
Authorization: Bearer {{ authorization }}
Prefer: code=400

[QueryStringParams]
status:


HTTP 400
