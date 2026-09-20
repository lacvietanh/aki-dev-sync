<template>
  <div :class="values?.[key]" />
  <div :class="items?.[index]" />
  <div :class="map?.[name]" />
  <div :class="state?.[kind]" />
  <div :class="theme?.[tone]" />
  <div :class="sizes?.[size]" />
  <div :class="flags?.[flag]" />
  <div :class="lookup?.[id]" />
  <div :class="classes?.[variant]" />
  <div :class="config?.[mode]" />
  <div :class="ptyApi?.pendingModifiers?.value?.[key]" />
</template>
