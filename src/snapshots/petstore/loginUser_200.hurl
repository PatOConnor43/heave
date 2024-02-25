GET {{ baseurl }}/user/login
Authorization: Bearer {{ authorization }}
Prefer: code=200

[QueryStringParams]
username:
password:

HTTP 200

[Asserts]
jsonpath "$" isString
