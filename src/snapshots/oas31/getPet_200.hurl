GET {{ baseurl }}/pets/{{petId}}
Authorization: Bearer {{ authorization }}
Prefer: code=200

HTTP 200

[Asserts]
jsonpath "$" isCollection
#jsonpath "$.age" isInteger
#jsonpath "$.aliases" isCollection
#jsonpath "$.aliases[0]" isString
#jsonpath "$.code" isInteger
#jsonpath "$.code" isString
jsonpath "$.id" isInteger
#jsonpath "$.metadata" isCollection
#jsonpath "$.metadata.source" isString
jsonpath "$.name" isString
#jsonpath "$.tag" isString
#jsonpath "$.weight" isNumber
