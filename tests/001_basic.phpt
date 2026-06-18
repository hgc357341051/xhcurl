--TEST--
XHCurl 基础功能测试：类加载和版本号
--SKIPIF--
<?php if (!extension_loaded('xhcurl')) echo 'skip xhcurl extension not loaded'; ?>
--FILE--
<?php
// 测试扩展是否加载
var_dump(extension_loaded('xhcurl'));

// 测试版本号函数
var_dump(xhcurl_version());

// 测试版本号常量
var_dump(XHCURL_VERSION);

// 测试 XHCurl 类是否可用
var_dump(class_exists('XHCurl'));

// 测试 XHRequest 类是否可用
var_dump(class_exists('XHRequest'));

// 测试 XHResponse 类是否可用
var_dump(class_exists('XHResponse'));

// 测试 XHMulti 类是否可用
var_dump(class_exists('XHMulti'));

// 测试 XHThreadPool 类是否可用
var_dump(class_exists('XHThreadPool'));

// 测试异常类是否可用
var_dump(class_exists('XHCurlException'));

// 测试 XHCurlException 继承关系
var_dump(is_subclass_of('XHCurlException', 'Exception'));
?>
--EXPECT--
bool(true)
string(5) "1.0.0"
string(5) "1.0.0"
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
