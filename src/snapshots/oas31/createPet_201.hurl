POST {{ baseurl }}/pets
Authorization: Bearer {{ authorization }}
Prefer: code=201
{
  "age": 0,
  "code": 0,
  "name": "",
  "tag": ""
}
HTTP 201

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
