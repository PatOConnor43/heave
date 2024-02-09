GET {{ baseurl }}/pets
Authorization: Bearer {{ authorization }}
Prefer: code=200

[QueryStringParams]
limit: 

HTTP 200

[Asserts]
jsonpath "$" isCollection

