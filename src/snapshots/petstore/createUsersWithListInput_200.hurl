POST {{ baseurl }}/user/createWithList
Authorization: Bearer {{ authorization }}
Prefer: code=200
[
  {
    "email": "",
    "firstName": "",
    "id": 0,
    "lastName": "",
    "password": "",
    "phone": "",
    "userStatus": 0,
    "username": ""
  }
]
HTTP 200

[Asserts]
jsonpath "$" isCollection
#jsonpath "$.id" isInteger
#jsonpath "$.username" isString
#jsonpath "$.firstName" isString
#jsonpath "$.lastName" isString
#jsonpath "$.email" isString
#jsonpath "$.password" isString
#jsonpath "$.phone" isString
#jsonpath "$.userStatus" isInteger
