GET {{ baseurl }}/pet/findByStatus
Authorization: Bearer {{ authorization }}
Prefer: code=200

[QueryStringParams]
status: 

HTTP 200

[Asserts]
jsonpath "$" isCollection

