GET {{ baseurl }}/pets/{{petId}}
Authorization: Bearer {{ authorization }}
Prefer: code=200

HTTP 200

[Asserts]
jsonpath "$" isCollection
jsonpath "$.id" isInteger
jsonpath "$.name" isString
jsonpath "$.tag" isString

