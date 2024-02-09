GET {{ baseurl }}/pet/{{petId}}
Authorization: Bearer {{ authorization }}
Prefer: code=200

HTTP 200

[Asserts]
jsonpath "$" isCollection
jsonpath "$.id" isInteger
jsonpath "$.name" isString
jsonpath "$.category" isCollection
jsonpath "$.category.id" isInteger
jsonpath "$.category.name" isString
jsonpath "$.photoUrls" isCollection
jsonpath "$.tags" isCollection
jsonpath "$.status" isString

